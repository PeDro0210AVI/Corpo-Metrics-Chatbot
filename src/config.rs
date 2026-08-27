use anyhow::{Context, Result};

/// Runtime configuration, resolved from environment variables (optionally
/// loaded from a `.env` file in the working directory).
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub mcp_server_name: String,
    pub mcp_server_url: String,
    pub mcp_auth_token: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Best-effort: pick up a local .env file if present. Missing file is fine.
        let _ = dotenvy::dotenv();

        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY is not set (export it, or put it in a .env file)")?;

        let model = std::env::var("CLAUDE_MODEL").unwrap_or_else(|_| "claude-opus-5".to_string());

        let max_tokens = std::env::var("CLAUDE_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        let mcp_server_name =
            std::env::var("MCP_SERVER_NAME").unwrap_or_else(|_| "pedro021-mcp".to_string());

        let mcp_server_url = std::env::var("MCP_SERVER_URL")
            .unwrap_or_else(|_| "https://mcp.pedro021.com/mcp".to_string());

        let mcp_auth_token = std::env::var("MCP_AUTH_TOKEN").ok();

        Ok(Self {
            api_key,
            model,
            max_tokens,
            mcp_server_name,
            mcp_server_url,
            mcp_auth_token,
        })
    }
}
