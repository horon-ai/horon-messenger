# @horon-ai/messenger-server-mcp

Installs the horon-messenger HTTP API server and the MCP server together. This
package has no binary of its own; it only pulls both, so `horon-messenger-server`
and `horon-messenger-mcp` are on the path after install.

```bash
npm install -g @horon-ai/messenger-server-mcp
# or
bun add -g @horon-ai/messenger-server-mcp
```

Installing `@horon-ai/messenger-mcp` on its own also works and is runnable with
`npx`/`bunx`; this package exists for deployments that want both in one step.
