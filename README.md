# horon-messenger

One bridge for every messenger, written in Rust. KakaoTalk, Discord, Slack,
Telegram, WhatsApp, LINE, Instagram, iMessage, Channel Talk and anything else,
behind a single provider contract and four surfaces: a CLI, an SDK, an MCP server
and an HTTP API server, plus a terminal UI.

Agent Messenger ([agent-messenger](https://github.com/agent-messenger/agent-messenger))
proved the shape: your agent messages **as you**, not as a bot, with credentials
taken from the desktop app or signed in through the platform's own flow. This
project rebuilds that idea in Rust and splits it into a workspace where each
platform is an independent adapter and each surface is a thin consumer of one SDK.

## Status

The contract, the four surfaces and the provider layout are in place and build.
Thirteen providers are wired in; KakaoTalk is the one remaining skeleton.

| Provider | Crate | State |
|----------|-------|-------|
| Mock | `crates/providers/mock` | Complete: in-memory, answers every operation |
| KakaoTalk | `crates/providers/kakaotalk` | Skeleton: capabilities declared, operations `NotImplemented` |
| Discord | `crates/providers/discord` | Implemented: HTTP REST |
| Channel Talk | `crates/providers/channeltalk` | Implemented: HTTP REST |
| Telegram Bot | `crates/providers/telegrambot` | Implemented: HTTP |
| WhatsApp Bot | `crates/providers/whatsappbot` | Implemented: HTTP (`whoami` and `send_message`) |
| Slack Bot | `crates/providers/slackbot` | Implemented: HTTP |
| Slack | `crates/providers/slack` | Implemented: HTTP |
| iMessage | `crates/providers/imessage` | Implemented: macOS, `imsg` sidecar |
| Telegram | `crates/providers/telegram` | Implemented: TDLib loaded at run time |
| WhatsApp | `crates/providers/whatsapp` | Implemented: Node sidecar (Baileys) |
| LINE | `crates/providers/line` | Implemented: Node sidecar (linejs) |
| Instagram | `crates/providers/instagram` | Implemented: HTTP session cookie |

KakaoTalk declares its capabilities and returns `NotImplemented` until its
implementation moves here from `~/Desktop/dev/alpox/kakaotalk-bridge`. A skeleton
returns `NotImplemented` for every operation it advertises, so a caller can tell
"not built yet" from "never supported" (`Unsupported`). The per-provider auth,
transport and prerequisites are in [docs/providers.md](docs/providers.md).

## Workspace

```
crates/
├── core/                     horon-messenger-core     Provider contract, domain model, config, credentials, wire envelope
├── sdk/                      horon-messenger-sdk      The public client over every provider a build enabled
├── cli/                      horon-messenger          Command line interface
├── mcp/                      horon-messenger-mcp      MCP server (stdio)
├── server/                   horon-messenger-server   HTTP API server
├── ui/                       horon-messenger-tui      Terminal UI
└── providers/
    ├── common/               sidecar JSON-RPC client and shared helpers
    ├── mock/                 in-memory provider for development and tests
    ├── kakaotalk/            skeleton
    ├── discord/              HTTP REST
    ├── channeltalk/          HTTP REST
    ├── telegrambot/          HTTP (Bot API)
    ├── whatsappbot/          HTTP (Cloud API)
    ├── slackbot/             HTTP (Web API)
    ├── slack/                HTTP (Web API)
    ├── imessage/             macOS, imsg sidecar
    ├── telegram/             TDLib loaded at run time
    ├── whatsapp/             Node sidecar (Baileys)
    ├── line/                 Node sidecar (linejs)
    └── instagram/            HTTP session cookie
```

Every surface depends only on `sdk`; `sdk` depends only on `core` and the
provider crates its features enabled. A provider depends only on `core`. That is
the whole dependency direction, and it never bends back.

## Install

Every surface ships as a prebuilt binary through npm, so no Rust toolchain is
needed:

```bash
npx @horon-ai/messenger-cli providers         # CLI
npx @horon-ai/messenger-mcp                   # MCP server (stdio)
npx @horon-ai/messenger-server --port 8787    # HTTP API server
```

`bunx` and `bun add -g` work the same way, and `@horon-ai/messenger` runs the
CLI too, so `npx @horon-ai/messenger daemon start` works after a one-line run.
Each surface installs on its own, in the server-anchored combinations, or all at
once:

| Deploy | Install |
|--------|---------|
| Everything (cli + mcp + server + tui) | `npm install -g @horon-ai/messenger` |
| CLI only | `npm install -g @horon-ai/messenger-cli` |
| MCP only | `npm install -g @horon-ai/messenger-mcp` |
| Server only | `npm install -g @horon-ai/messenger-server` |
| Server + MCP | `npm install -g @horon-ai/messenger-server-mcp` |
| Server + TUI | `npm install -g @horon-ai/messenger-server-tui` |
| Server + MCP + TUI | `npm install -g @horon-ai/messenger-server-mcp-tui` |

`cli` and `mcp` also run straight from the registry with `npx` or `bunx`:

```bash
npx @horon-ai/messenger-cli providers
npx @horon-ai/messenger daemon start
bunx @horon-ai/messenger-mcp
```

The npm packages are thin wrappers:
on install they pull the prebuilt binary for your platform
(`@horon-ai/messenger-cli-darwin-arm64` and so on) and run it, forwarding stdio
and the exit code. The wrapper source lives in [`npm/`](npm/).

The Rust-native paths stay available for a smaller provider set:

```bash
cargo install --path crates/cli --features discord,telegram
cargo install horon-messenger-cli                # once published to crates.io
```

The published npm binaries are built with every provider enabled. To install
only some providers, use the cargo path with the matching `--features`.
`core` and `sdk` are libraries, not binaries: add them from Rust with
`cargo add horon-messenger-sdk --features discord,telegram`.

## Docs

Design and per-surface reference live in [`docs/`](docs/README.md):

| Doc | Covers |
|-----|--------|
| [docs/CLI.md](docs/CLI.md) | The `horon-messenger` CLI: commands, daemon, setup, update |
| [docs/MCP.md](docs/MCP.md) | The MCP server and its tools |
| [docs/SERVER.md](docs/SERVER.md) | The HTTP API server and its routes |
| [docs/UI.md](docs/UI.md) | The terminal UI |
| [docs/SDK.md](docs/SDK.md) | The `Client` library surface |
| [docs/CORE.md](docs/CORE.md) | The provider contract, model and envelope |
| [docs/architecture.md](docs/architecture.md), [docs/providers.md](docs/providers.md) | Layers and every provider |

## Build

```bash
cargo build --workspace                     # every crate, default features (mock)
cargo build --workspace --all-features      # every provider wired in
cargo test --workspace                      # unit and integration tests
cargo clippy --workspace --all-targets      # lints
```

Every surface controls its own provider set. Each defaults to `mock` and takes
one feature per platform (`discord`, `telegram`, `channeltalk`, `whatsapp`,
`line`, `instagram` and the rest), plus `all` for every provider:

```bash
cargo build -p horon-messenger-cli --features discord
cargo build -p horon-messenger-mcp --features all
cargo build -p horon-messenger-server --features telegram,discord
cargo build -p horon-messenger-ui --features all
```

The toolchain is pinned in `rust-toolchain.toml` (1.97.1, edition 2024).

## Surfaces

### CLI

```bash
cargo run -p horon-messenger-cli -- providers
cargo run -p horon-messenger-cli -- chats mock --pretty
cargo run -p horon-messenger-cli -- messages mock general --pretty
cargo run -p horon-messenger-cli -- send mock general "hello"
cargo run -p horon-messenger-cli -- auth status mock
```

Beyond the message commands, the CLI manages the local install:

```bash
horon-messenger setup                 # prepare the config dir and report provider readiness
horon-messenger daemon start          # run the HTTP API server in the background
horon-messenger daemon status         # is it running, and on which url
horon-messenger daemon restart        # stop then start
horon-messenger daemon stop           # stop the background server
horon-messenger update --check        # is a newer release published
horon-messenger update                # install the latest release (npm install path)
```

`daemon` finds `horon-messenger-server` next to the CLI, on `PATH`, or at
`HORON_MESSENGER_SERVER`. It writes `server.pid` and `server.log` under the
config dir and probes the port for liveness. `update` reads the npm registry for
the installed package and installs the latest with the package manager that ran
the CLI (npm, bun, pnpm or yarn); a build that did not come from npm reports the
available version instead of overwriting itself.

Output is a JSON envelope on stdout: `{"ok": true, "data": ..., "meta": ...}` on
success, `{"ok": false, "error": {"code", "message", "retryable"}, "meta": ...}`
on failure. `--pretty` indents it; the exit code is non-zero when `ok` is false.

### MCP server

```bash
cargo run -p horon-messenger-mcp
```

Speaks MCP over stdio and exposes `list_providers`, `whoami`, `list_chats`,
`list_messages`, `send_message` and `list_members`.

### HTTP API server

```bash
cargo run -p horon-messenger-server -- --host 127.0.0.1 --port 8787
curl localhost:8787/v1/providers
curl 'localhost:8787/v1/providers/mock/chats'
curl -X POST localhost:8787/v1/providers/mock/chats/general/messages \
  -H 'content-type: application/json' -d '{"text":"hello"}'
```

The server binds to loopback by default. It returns the same envelope as the CLI.

### Terminal UI

```bash
cargo run -p horon-messenger-ui -- --provider mock
cargo run -p horon-messenger-ui -- --provider mock --snapshot   # non-interactive
```

`j`/`k` or the arrow keys move between chats, `q` quits.

## Configuration

Credentials and per-provider state live under
`~/.config/horon-messenger/<provider>/`. Set `HORON_MESSENGER_CONFIG_DIR` to
relocate everything (sandboxes, CI, non-default homes). Files are owner-only
(`0700` directories, `0600` files) and written atomically.

## Adding a provider

Every provider above followed the same steps, so adding the next platform is the
same drill:

1. Add the crate under `crates/providers/<name>/` and list it in the root
   `Cargo.toml` `members`.
2. Add a `horon-messenger-provider-<name>` entry to `[workspace.dependencies]`.
3. Add an optional dependency and a feature in `crates/sdk/Cargo.toml`, and
   include it in `all`.
4. Register it in `crates/sdk/src/lib.rs::register_builtin` behind
   `#[cfg(feature = "<name>")]`.
5. Add a `ProviderKind` variant if the platform is new.

A provider that needs a helper process (iMessage, WhatsApp, LINE) also ships a
sidecar under `crates/providers/<name>/sidecar/` and speaks line-delimited
JSON-RPC 2.0 through `horon-messenger-provider-common`.

Full detail is in [docs/providers.md](docs/providers.md), [docs/architecture.md](docs/architecture.md)
and the per-surface docs under [docs/](docs/README.md).

## License

MIT. See [LICENSE](LICENSE).
