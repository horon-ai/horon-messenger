# @horon-ai/messenger-server-tui

Installs the horon-messenger HTTP API server and the terminal UI together. The
server binary comes from `@horon-ai/messenger-server`; this package carries the
TUI launcher and pulls the TUI platform binary. After install both
`horon-messenger-server` and `horon-messenger-tui` are on the path.

```bash
npm install -g @horon-ai/messenger-server-tui
# or
bun add -g @horon-ai/messenger-server-tui
```

The TUI has no standalone package; this bundle and
`@horon-ai/messenger-server-mcp-tui` are the only ways to get it.
