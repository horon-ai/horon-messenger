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
| `crates/core` | `horon-messenger-core` | `Provider` trait, domain model, `Capabilities`, `ConfigPaths`, `CredentialStore`, `Envelope` |
| `crates/sdk` | `horon-messenger-sdk` | `Client` / `ClientBuilder`; assembles the registry of enabled providers |
| `crates/cli` | `horon-messenger-cli` | `horon-messenger` binary (clap) |
| `crates/mcp` | `horon-messenger-mcp` | `horon-messenger-mcp` binary (rmcp, stdio) |
| `crates/server` | `horon-messenger-server` | `horon-messenger-server` binary (axum) |
| `crates/ui` | `horon-messenger-ui` | `horon-messenger-tui` binary (ratatui) |
| `crates/providers/*` | `horon-messenger-provider-*` | One crate per platform |

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
| CI, release automation | `.github/workflows/{ci,release,release-main}.yml`, `scripts/release/` |

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
node --test scripts/release
actionlint
```

## Notes

- Rust 2024, toolchain pinned to 1.97.1 in `rust-toolchain.toml`.
- Feature flags on `sdk`: `mock` (default), `kakaotalk`, `discord`, `telegram`,
  `channeltalk`, `all`. Provider crates are always workspace members, so
  `cargo build --workspace` compiles them even when `sdk` does not re-export them.
- The HTTP server binds to loopback by default; do not widen that without a reason.
- Releases are cut from `main` (integration on `dev`). CI (`.github/workflows/ci.yml`)
  runs on a push to every branch and on every pull request. A push to `main` that
  passes CI triggers `release-main.yml`, which bumps the minor version in
  `Cargo.toml`, tags `vX.Y.Z`, fast-forwards `dev` to the release commit so the two
  branches stay identical, and dispatches `release.yml`; `release.yml` publishes to
  npm when `NPM_TOKEN` is set and always creates the GitHub Release. The workspace
  version in the root `Cargo.toml` is the single source of truth;
  `scripts/release/bump.mjs` rewrites it and `npm/scripts/sync-version.mjs`
  propagates it to the npm manifests.
