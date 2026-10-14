# horon-messenger docs

Design and per-surface reference. The root [README](../README.md) is the entry
point; these files go deeper.

## Surfaces

| Doc | Surface | Binary |
|-----|---------|--------|
| [CLI.md](CLI.md) | Command line interface | `horon-messenger` |
| [MCP.md](MCP.md) | MCP server over stdio | `horon-messenger-mcp` |
| [SERVER.md](SERVER.md) | HTTP API server | `horon-messenger-server` |
| [UI.md](UI.md) | Terminal UI | `horon-messenger-tui` |

## Libraries

| Doc | Crate | Role |
|-----|-------|------|
| [SDK.md](SDK.md) | `horon-messenger-sdk` | `Client` over every enabled provider |
| [CORE.md](CORE.md) | `horon-messenger-core` | Provider contract, model, envelope, config |

## Design

| Doc | Contents |
|-----|----------|
| [architecture.md](architecture.md) | Layers, the provider contract, capabilities, envelope, config, timestamps |
| [providers.md](providers.md) | Every provider: auth, transport, prerequisites; how to add one |

## Layout

```
surfaces   cli ─┐
           mcp ─┤
         server ─┼──► sdk ──► core ◄── providers (one crate per platform)
            ui ─┘
```

Every surface depends only on `sdk`; `sdk` depends only on `core` and the
provider crates its features enabled; a provider depends only on `core`. See
[architecture.md](architecture.md) for the full picture.
