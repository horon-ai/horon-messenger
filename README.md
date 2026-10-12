# hyper-messenger

One bridge for every messenger, written in Rust. KakaoTalk, Discord, Telegram,
Channel Talk and anything else, behind a single provider contract and four
surfaces: a CLI, an SDK, an MCP server and an HTTP API server, plus a terminal UI.

Agent Messenger ([agent-messenger](https://github.com/agent-messenger/agent-messenger))
proved the shape: your agent messages **as you**, not as a bot, with credentials
taken from the desktop app or signed in through the platform's own flow. This
project rebuilds that idea in Rust and splits it into a workspace where each
platform is an independent adapter and each surface is a thin consumer of one SDK.

## Status

This is the project scaffold. The contract, the four surfaces and the provider
layout are in place and build; the platform adapters are being filled in.

| Provider | Crate | State |
|----------|-------|-------|
| Mock | `crates/providers/mock` | Complete: in-memory, answers every operation |
| KakaoTalk | `crates/providers/kakaotalk` | Skeleton: capabilities declared, operations `NotImplemented` |
| Discord | `crates/providers/discord` | Skeleton |
| Telegram | `crates/providers/telegram` | Skeleton |
| Channel Talk | `crates/providers/channeltalk` | Skeleton |

The KakaoTalk adapter is the first to be completed: its implementation moves here
from `~/Desktop/dev/alpox/kakaotalk-bridge`. A skeleton returns
`NotImplemented` for every operation it advertises, so a caller can tell "not
built yet" from "never supported" (`Unsupported`).

## Workspace

```
crates/
├── core/                     hyper-messenger-core     Provider contract, domain model, config, credentials, wire envelope
├── sdk/                      hyper-messenger-sdk      The public client over every provider a build enabled
├── cli/                      hyper-messenger          Command line interface
├── mcp/                      hyper-messenger-mcp      MCP server (stdio)
├── server/                   hyper-messenger-server   HTTP API server
├── ui/                       hyper-messenger-tui      Terminal UI
└── providers/
    ├── mock/                 in-memory provider for development and tests
    ├── kakaotalk/
    ├── discord/
    ├── telegram/
    └── channeltalk/
```

Every surface depends only on `sdk`; `sdk` depends only on `core` and the
provider crates its features enabled. A provider depends only on `core`. That is
the whole dependency direction, and it never bends back.

## Build

```bash
cargo build --workspace                     # every crate, default features (mock)
cargo build --workspace --all-features      # every provider wired in
cargo test --workspace                      # unit and integration tests
cargo clippy --workspace --all-targets      # lints
```

The toolchain is pinned in `rust-toolchain.toml` (1.97.1, edition 2024).

## Surfaces

### CLI

```bash
cargo run -p hyper-messenger-cli -- providers
cargo run -p hyper-messenger-cli -- chats mock --pretty
cargo run -p hyper-messenger-cli -- messages mock general --pretty
cargo run -p hyper-messenger-cli -- send mock general "hello"
cargo run -p hyper-messenger-cli -- auth status mock
```

Output is a JSON envelope on stdout: `{"ok": true, "data": ..., "meta": ...}` on
success, `{"ok": false, "error": {"code", "message", "retryable"}, "meta": ...}`
on failure. `--pretty` indents it; the exit code is non-zero when `ok` is false.

### MCP server

```bash
cargo run -p hyper-messenger-mcp
```

Speaks MCP over stdio and exposes `list_providers`, `whoami`, `list_chats`,
`list_messages`, `send_message` and `list_members`.

### HTTP API server

```bash
cargo run -p hyper-messenger-server -- --host 127.0.0.1 --port 8787
curl localhost:8787/v1/providers
curl 'localhost:8787/v1/providers/mock/chats'
curl -X POST localhost:8787/v1/providers/mock/chats/general/messages \
  -H 'content-type: application/json' -d '{"text":"hello"}'
```

The server binds to loopback by default. It returns the same envelope as the CLI.

### Terminal UI

```bash
cargo run -p hyper-messenger-ui -- --provider mock
cargo run -p hyper-messenger-ui -- --provider mock --snapshot   # non-interactive
```

`j`/`k` or the arrow keys move between chats, `q` quits.

## Configuration

Credentials and per-provider state live under
`~/.config/hyper-messenger/<provider>/`. Set `HYPER_MESSENGER_CONFIG_DIR` to
relocate everything (sandboxes, CI, non-default homes). Files are owner-only
(`0700` directories, `0600` files) and written atomically.

## Adding a provider

See [docs/providers.md](docs/providers.md) and
[docs/architecture.md](docs/architecture.md).

## License

MIT. See [LICENSE](LICENSE).
