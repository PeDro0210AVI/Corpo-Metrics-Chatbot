# Corpo Metrics Chatbot

A terminal UI (TUI) chat client, written in Rust, that talks to Claude
through the Anthropic Messages API and gives it live access to a remote
MCP (Model Context Protocol) server at `mcp.pedro021.com`.

Claude connects to the MCP server directly via Anthropic's server-side
[MCP connector](https://platform.claude.com/docs/en/agents-and-tools/tool-use/model-context-protocol) —
this client only needs your Anthropic API key and the MCP server's URL; it
does not implement the MCP protocol itself.

## Requirements

- [Nix](https://nixos.org/) with flakes enabled (the project is built and
  run through the provided `flake.nix`), **or** a local Rust toolchain
  (1.75+) if you'd rather not use Nix.
- An Anthropic API key ([console.anthropic.com](https://console.anthropic.com)).
- Network access to `mcp.pedro021.com`.

## Setup

1. Copy the example environment file and fill in your API key:

   ```sh
   cp .env.example .env
   $EDITOR .env
   ```

   At minimum you need `ANTHROPIC_API_KEY` set. `MCP_SERVER_URL` already
   defaults to `https://mcp.pedro021.com/mcp` — only change it if the
   server exposes a different path, or set `MCP_AUTH_TOKEN` if it requires
   a bearer token.

2. Enter the dev shell and build:

   ```sh
   nix develop
   cargo build
   ```

## Running

```sh
nix develop -c cargo run
# or, from inside `nix develop`:
cargo run

# or build once and run the packaged binary:
nix build
./result/bin/corpo-metrics-chatbot
```

Environment variables are read from the process environment first and
fall back to a `.env` file in the current directory.

## Usage

- Type your message and press **Enter** to send it.
- While Claude is replying, the input box is locked; incoming text streams
  in live, and a status line shows when Claude is calling a tool on the
  MCP server.
- **↑ / ↓** scroll the conversation one line at a time, **PageUp / PageDown**
  scroll by a page, **End** jumps back to the latest message.
- **Esc** or **Ctrl-C** quits.

## Configuration reference

All configuration is via environment variables (see `.env.example`):

| Variable            | Required | Default                          | Description                                   |
| ------------------- | -------- | --------------------------------- | ---------------------------------------------- |
| `ANTHROPIC_API_KEY`  | yes      | —                                  | Your Anthropic API key                         |
| `CLAUDE_MODEL`       | no       | `claude-opus-5`                    | Model ID to send requests to                   |
| `CLAUDE_MAX_TOKENS`  | no       | `4096`                             | Max output tokens per reply                    |
| `MCP_SERVER_URL`     | no       | `https://mcp.pedro021.com/mcp`     | URL of the remote MCP server                   |
| `MCP_SERVER_NAME`    | no       | `pedro021-mcp`                     | Internal name Claude uses to refer to the server |
| `MCP_AUTH_TOKEN`     | no       | —                                  | Bearer token for the MCP server, if required   |

## Project layout

```
src/
├── main.rs    entry point, terminal setup, event loop
├── app.rs     application state (conversation history, input, scroll)
├── ui.rs      ratatui rendering
├── claude.rs  Claude API client (SSE streaming + MCP connector wiring)
└── config.rs  environment-variable configuration
```

## How it works

Every request sent to `POST /v1/messages` includes:

```json
{
  "model": "claude-opus-5",
  "mcp_servers": [
    { "type": "url", "name": "pedro021-mcp", "url": "https://mcp.pedro021.com/mcp" }
  ],
  "tools": [
    { "type": "mcp_toolset", "mcp_server_name": "pedro021-mcp" }
  ],
  "stream": true
}
```

with the beta header `anthropic-beta: mcp-client-2025-11-20`. Anthropic
handles the MCP connection server-side; the client streams back Claude's
text plus `mcp_tool_use` / `mcp_tool_result` blocks, which the TUI surfaces
as status lines while the reply streams in.

## License

MIT
