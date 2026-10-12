# AGENTS.md

Knowledge base for agents working in this repository.

## What this is

A Rust workspace that bridges many messengers behind one provider contract. Four
surfaces (CLI, MCP server, HTTP API server, TUI) consume one SDK, and each
platform is an independent adapter crate. It is the Rust counterpart of
`agent-messenger`, and the KakaoTalk adapter is being ported here from
`kakaotalk-bridge`.

## Layout

| Path | Crate | Role |
|------|-------|------|
| `crates/core` | `hyper-messenger-core` | `Provider` trait, domain model, `Capabilities`, `ConfigPaths`, `CredentialStore`, `Envelope` |
| `crates/sdk` | `hyper-messenger-sdk` | `Client` / `ClientBuilder`; assembles the registry of enabled providers |
| `crates/cli` | `hyper-messenger-cli` | `hyper-messenger` binary (clap) |
| `crates/mcp` | `hyper-messenger-mcp` | `hyper-messenger-mcp` binary (rmcp, stdio) |
| `crates/server` | `hyper-messenger-server` | `hyper-messenger-server` binary (axum) |
| `crates/ui` | `hyper-messenger-ui` | `hyper-messenger-tui` binary (ratatui) |
| `crates/providers/*` | `hyper-messenger-provider-*` | One crate per platform |

Dependency direction: `providers -> core`, `sdk -> core + providers`, `surfaces ->
sdk`. Nothing depends on a surface.

## Where to look

| Task | Location |
|------|----------|
| Add or change a provider operation | `crates/core/src/provider.rs` (trait), then the provider crate |
| Add a provider | `crates/providers/<name>/`, register in `crates/sdk/src/lib.rs` `register_builtin`, add a workspace member and a feature |
| Change the wire format | `crates/core/src/envelope.rs` (all surfaces share it) |
| Add a CLI command | `crates/cli/src/main.rs` |
| Add an MCP tool | `crates/mcp/src/main.rs` |
| Add an HTTP route | `crates/server/src/main.rs` |
| Config or credential layout | `crates/core/src/config.rs`, `crates/core/src/credential.rs` |

## Conventions

- The default provider method returns an error: `NotImplemented` when the
  capability is advertised in `capabilities()`, `Unsupported` when it is not. A
  provider overrides only what it implements; never re-add a default body.
- Errors are typed in `core` (`thiserror`) and carry the `ProviderKind`. No
  `anyhow` inside library crates; `anyhow` is only allowed at binary edges.
- Every surface returns the `Envelope` from `core`; do not invent a second shape.
- IDs crossing a boundary are strings.
- Timestamps come from `core::now_rfc3339` so every surface stamps identically.
- Credentials live under `<config>/<provider>/accounts/<account>.json`, owner-only
  and written atomically through `write_private_atomic`.
- Public items get a one-line doc comment. Do not add narration comments.

## Commands

```bash
cargo build --workspace
cargo build --workspace --all-features
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Notes

- Rust 2024, toolchain pinned to 1.97.1 in `rust-toolchain.toml`.
- Feature flags on `sdk`: `mock` (default), `kakaotalk`, `discord`, `telegram`,
  `channeltalk`, `all`. Provider crates are always workspace members, so
  `cargo build --workspace` compiles them even when `sdk` does not re-export them.
- The HTTP server binds to loopback by default; do not widen that without a reason.
