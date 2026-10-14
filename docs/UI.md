# Terminal UI

`horon-messenger-tui` is a ratatui terminal UI over the [`Client`](SDK.md). It
shows a provider's chats on the left and the selected chat's messages on the
right, and it can print a plain-text snapshot for scripts.

- Crate: `crates/ui` (`horon-messenger-ui`)
- Binary: `horon-messenger-tui`
- Framework: ratatui + crossterm
- npm: no standalone package; the binary ships in the `@horon-ai/messenger-server-tui`
  and `@horon-ai/messenger-server-mcp-tui` bundles (see [npm/README](../npm/README.md))

## Run

```bash
cargo run -p horon-messenger-ui -- --provider mock
cargo run -p horon-messenger-ui -- --provider mock --snapshot
npx @horon-ai/messenger-tui --provider mock
```

| Flag | Default | Description |
|------|---------|-------------|
| `--provider` | `mock` | Provider to browse |
| `--snapshot` | off | Print a text snapshot and exit instead of starting the TUI |

## Keys

| Key | Action |
|-----|--------|
| `j` / Down | Select the next chat |
| `k` / Up | Select the previous chat |
| `q` / Esc | Quit |

The chat list wraps around at the ends. Selecting a chat reloads its messages.

## Snapshot mode

`--snapshot` prints the chat list and the first chat's messages as text, then
exits — useful for a quick check without a terminal UI:

```
Mock: 3 chats
  general          group    General
  team             group    Team Standup
  ...
General (2 messages):
  alice: ...
```

## Status

Implemented: chat list, message view, keyboard navigation, snapshot mode. The UI
is read-only — it lists and views; sending and other operations stay with the CLI
and the HTTP server. It loads chats and messages once per selection; there is no
live refresh or push yet.
