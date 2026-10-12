# brief: port Discord and Channel Talk providers to Rust

Port the Discord and Channel Talk platform clients from the reference TypeScript
repo (github.com/agent-messenger/agent-messenger, cloned at
/tmp/agent-messenger-ref) into the Rust workspace at
/Users/alpox/Desktop/dev/alpox/hyper-messenger, so both providers implement the
core `Provider` contract with real HTTP clients, credentials, and read/send.

## Deliverables

- `crates/providers/discord`: Discord REST v10 client (token auth,
  `X-Super-Properties` header, rate-limit buckets, retries), `Provider` impl
  (whoami, list_chats, get_chat, list_messages, search_messages, send_message,
  list_members, mark_read, snapshot), `AuthProvider` impl, credentials under
  `<config>/discord`.
- `crates/providers/channeltalk`: Channel Talk desk-api client (cookie auth,
  rate-limit, retries), `Provider` impl (whoami, list_chats, get_chat,
  list_messages, search_messages, send_message, list_members, snapshot),
  `AuthProvider` impl, credentials under `<config>/channeltalk`.
- Both reachable from the CLI, MCP server and HTTP server through the existing sdk.

## Scope bounds (assumed)

- Credentials load from config/env and can be set via `auth login`; desktop-app and
  browser token EXTRACTION (macOS Chromium cookie decryption) is OUT of scope.
- Discord gateway websocket listener, media/emoji/sticker upload, reactions,
  threads and user notes are OUT of scope for this pass.
- Base URLs are overridable by env var so behavior can be proven against a local
  mock HTTP server without live credentials.

## Success criteria (observable)

- Discord: CLI `chats discord` against a mock Discord API returns the seeded
  guild/channel list; `send discord <channel> <text>` posts and returns the created
  message; `messages discord <channel>` lists it back.
- Channel Talk: CLI `chats channeltalk` against a mock desk API returns the seeded
  channels; send + list round-trip.
- Both: `cargo test --workspace --all-features` green, clippy `-D warnings` clean,
  `cargo fmt --check` clean.
- End-to-end: the HTTP server and MCP expose both providers.

## When to stop

Both providers are implemented, wired, and proven by real-surface QA (CLI against a
mock HTTP server plus unit tests green), landed on `dev`.
