# @horon-ai/messenger

Installs every horon-messenger surface at once. This package has no binary of its
own; it depends on the CLI and on the Server + MCP + TUI bundle, so after install
all four binaries are on the path:

- `horon-messenger` (CLI)
- `horon-messenger-mcp` (MCP server, stdio)
- `horon-messenger-server` (HTTP API server)
- `horon-messenger-tui` (terminal UI)

```bash
npm install -g @horon-ai/messenger
# or
bun add -g @horon-ai/messenger
```

To install only some surfaces, use the smaller packages instead: the CLI is
`@horon-ai/messenger-cli`, and the server-anchored bundles are
`@horon-ai/messenger-server-mcp`, `@horon-ai/messenger-server-tui` and
`@horon-ai/messenger-server-mcp-tui`.
