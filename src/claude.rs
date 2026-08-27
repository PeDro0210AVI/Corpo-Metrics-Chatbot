use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Beta flag for the MCP connector: lets Claude call tools on a remote MCP
/// server directly from the Messages API (no local MCP client needed).
const MCP_CONNECTOR_BETA: &str = "mcp-client-2025-11-20";

/// Events streamed back from a single Claude API call, consumed by the UI loop.
#[derive(Debug, Clone)]
pub enum ApiEvent {
    /// A chunk of assistant text to append to the in-progress reply.
    TextDelta(String),
    /// Claude is invoking a tool on the connected MCP server.
    ToolUse { name: String },
    /// Result of an MCP tool call came back.
    ToolResult { is_error: bool },
    /// The turn finished. Carries the raw content blocks so the full
    /// assistant turn (including tool_use/tool_result blocks) can be
    /// replayed as history on the next request.
    Done { content: Vec<Value> },
    /// Something went wrong (network, API error, refusal, etc).
    Error(String),
}

pub struct ClaudeClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: u32,
    mcp_server_name: String,
    mcp_server_url: String,
    mcp_auth_token: Option<String>,
}

impl ClaudeClient {
    pub fn new(config: &Config) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            mcp_server_name: config.mcp_server_name.clone(),
            mcp_server_url: config.mcp_server_url.clone(),
            mcp_auth_token: config.mcp_auth_token.clone(),
        }
    }

    fn build_body(&self, messages: &[Value]) -> Value {
        let mut mcp_server = json!({
            "type": "url",
            "name": self.mcp_server_name,
            "url": self.mcp_server_url,
        });
        if let Some(token) = &self.mcp_auth_token {
            mcp_server["authorization_token"] = json!(token);
        }

        json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "stream": true,
            "mcp_servers": [mcp_server],
            "tools": [
                { "type": "mcp_toolset", "mcp_server_name": self.mcp_server_name }
            ],
            "messages": messages,
        })
    }

    /// Sends the full conversation history and streams the reply, forwarding
    /// `ApiEvent`s to `tx` as they arrive. Runs to completion on the calling
    /// task — callers spawn this on its own tokio task.
    pub async fn stream_reply(&self, messages: Vec<Value>, tx: UnboundedSender<ApiEvent>) {
        let body = self.build_body(&messages);

        let response = match self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", MCP_CONNECTOR_BETA)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let _ = tx.send(ApiEvent::Error(format!("request failed: {e}")));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());
            let _ = tx.send(ApiEvent::Error(format!("API error {status}: {text}")));
            return;
        }

        let mut byte_stream = response.bytes_stream();
        let mut buf = String::new();

        let mut finished_blocks: Vec<Value> = Vec::new();
        let mut current_block: Option<Value> = None;
        // Accumulators for the block currently being streamed. Which ones
        // apply depends on the block's type: "text" fills current_text via
        // text_delta, "thinking" fills current_text via thinking_delta and
        // current_signature via signature_delta, "mcp_tool_use"/"tool_use"
        // fills current_partial_json via input_json_delta.
        let mut current_text = String::new();
        let mut current_signature = String::new();
        let mut current_partial_json = String::new();

        while let Some(chunk) = byte_stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(ApiEvent::Error(format!("stream error: {e}")));
                    return;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));

            // SSE frames are separated by a blank line.
            while let Some(pos) = buf.find("\n\n") {
                let frame = buf[..pos].to_string();
                buf.drain(..pos + 2);

                let Some(data_line) = frame.lines().find_map(|l| l.strip_prefix("data: ")) else {
                    continue;
                };

                let Ok(event) = serde_json::from_str::<Value>(data_line) else {
                    continue;
                };

                let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");

                match event_type {
                    "content_block_start" => {
                        let block = event.get("content_block").cloned().unwrap_or(json!({}));
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");

                        match block_type {
                            "mcp_tool_use" => {
                                let name = block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool")
                                    .to_string();
                                let _ = tx.send(ApiEvent::ToolUse { name });
                            }
                            "mcp_tool_result" => {
                                let is_error = block
                                    .get("is_error")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                let _ = tx.send(ApiEvent::ToolResult { is_error });
                            }
                            _ => {}
                        }

                        current_text.clear();
                        current_signature.clear();
                        current_partial_json.clear();
                        current_block = Some(block);
                    }
                    "content_block_delta" => {
                        if let Some(delta) = event.get("delta") {
                            match delta.get("type").and_then(Value::as_str) {
                                Some("text_delta") => {
                                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                                        current_text.push_str(text);
                                        let _ = tx.send(ApiEvent::TextDelta(text.to_string()));
                                    }
                                }
                                Some("thinking_delta") => {
                                    if let Some(text) = delta.get("thinking").and_then(Value::as_str) {
                                        current_text.push_str(text);
                                    }
                                }
                                Some("signature_delta") => {
                                    if let Some(sig) = delta.get("signature").and_then(Value::as_str) {
                                        current_signature.push_str(sig);
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(fragment) =
                                        delta.get("partial_json").and_then(Value::as_str)
                                    {
                                        current_partial_json.push_str(fragment);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_stop" => {
                        if let Some(mut block) = current_block.take() {
                            match block.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    block["text"] = json!(current_text);
                                }
                                Some("thinking") => {
                                    // The API rejects a replayed thinking block whose
                                    // "thinking" field is missing, even if empty (the
                                    // default when display isn't "summarized") — always
                                    // set it explicitly. Only attach a signature if one
                                    // actually streamed back.
                                    block["thinking"] = json!(current_text);
                                    if !current_signature.is_empty() {
                                        block["signature"] = json!(current_signature);
                                    }
                                }
                                Some("mcp_tool_use") | Some("tool_use") => {
                                    if !current_partial_json.is_empty() {
                                        if let Ok(parsed) =
                                            serde_json::from_str::<Value>(&current_partial_json)
                                        {
                                            block["input"] = parsed;
                                        }
                                    }
                                }
                                _ => {}
                            }
                            finished_blocks.push(block);
                        }
                        current_text.clear();
                        current_signature.clear();
                        current_partial_json.clear();
                    }
                    "error" => {
                        let msg = event
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error")
                            .to_string();
                        let _ = tx.send(ApiEvent::Error(msg));
                        return;
                    }
                    "message_stop" => {
                        let _ = tx.send(ApiEvent::Done {
                            content: finished_blocks.clone(),
                        });
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Stream ended without an explicit message_stop (shouldn't normally
        // happen, but don't leave the UI hanging).
        let _ = tx.send(ApiEvent::Done {
            content: finished_blocks,
        });
    }
}
