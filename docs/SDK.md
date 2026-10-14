# SDK

`horon-messenger-sdk` is the public library surface. It assembles a
[`ProviderRegistry`](CORE.md) from the providers a build enabled and exposes
`Client`, the one entry point every surface (CLI, MCP server, HTTP server, TUI)
uses. It re-exports all of `horon-messenger-core`, so `use horon_messenger_sdk::*`
reaches the domain model, errors and the envelope too.

- Crate: `crates/sdk` (`horon-messenger-sdk`)
- Depends on: `horon-messenger-core` and the provider crates its features enabled

## Add it

```bash
cargo add horon-messenger-sdk --features discord,telegram
```

| Feature | Enables |
|---------|---------|
| `mock` (default) | The in-memory provider for development and tests |
| `kakaotalk`, `discord`, `telegram`, `channeltalk`, `telegrambot`, `whatsappbot`, `slackbot`, `slack`, `imessage`, `whatsapp`, `line`, `instagram` | One provider each |
| `all` | Every provider above |
| `schemars` | `JsonSchema` derives (used by the MCP server) |

A provider crate is always a workspace member, so `cargo build --workspace`
compiles it even when `sdk` does not re-export it. Only the features a build
enabled end up in the registry.

## Client

```rust
use horon_messenger_sdk::{Client, ChatQuery, MessageQuery, SendRequest, ProviderKind};

let client = Client::builder()
    .config_dir("/tmp/horon-config")   // optional; else the platform default
    .build()?;

for info in client.describe() {
    println!("{}: {:?}", info.name, info.capabilities);
}

let chats = client.list_chats(ProviderKind::Mock, &ChatQuery::default()).await?;
let page  = client.list_messages(ProviderKind::Mock, &chats[0].id, &MessageQuery::default()).await?;
let sent  = client.send_message(ProviderKind::Mock, &SendRequest {
    chat_id: chats[0].id.clone(),
    text: "hello".to_owned(),
    ..SendRequest::default()
}).await?;
```

### `Client` methods

| Method | Returns |
|--------|---------|
| `builder()` | A `ClientBuilder` |
| `registry()` | The `ProviderRegistry` of enabled providers |
| `paths()` | The resolved `ConfigPaths` |
| `provider(kind)` | The `DynProvider` for a kind, or `NotRegistered` |
| `describe()` | `Vec<ProviderInfo>`: each enabled provider with its capabilities |
| `whoami(kind)` | The authenticated `Profile` |
| `list_chats(kind, &ChatQuery)` | `Vec<Chat>` |
| `list_messages(kind, chat_id, &MessageQuery)` | `MessagePage` |
| `send_message(kind, &SendRequest)` | `SendResult` |
| `list_members(kind, chat_id)` | `Vec<Member>` |
| `snapshot(kind)` | `Snapshot` (profile + chats) |
| `get_chat(kind, chat_id)` | One `Chat` by id |
| `search_messages(kind, chat_id?, &MessageQuery)` | `MessagePage` of matching messages |
| `mark_read(kind, chat_id)` | `()` |
| `listen(kind, chat_id?)` | An `EventStream` of provider events |

`Client` exposes the operations the surfaces use. The full [`Provider`](CORE.md)
contract also has `send_media`; reach it through `client.provider(kind)?` when
needed.

### `ClientBuilder`

| Method | Effect |
|--------|--------|
| `new()` | Empty registry, default config paths |
| `register(provider)` | Add a provider (in addition to the built-ins) |
| `config_dir(dir)` | Set the config dir |
| `config_paths(paths)` | Set a full `ConfigPaths` |
| `build()` | Register the built-ins for this build and return `Client` |

`default_client()` is shorthand for `Client::builder().build()`.

## Custom providers

`build()` always registers the built-ins this build enabled, then any provider
added with `register`. Register a hand-built provider (or a test double) without
changing features:

```rust
let client = Client::builder()
    .register(std::sync::Arc::new(MyProvider::new()))
    .build()?;
```

A provider is any `Arc<dyn Provider>`; implement the [`Provider`](CORE.md) trait
and it works in every surface.

## Errors

Every method returns `Result<T, Error>` where `Error` is `core::Error`. Match on
`error.code()` for a stable class (`NOT_REGISTERED`, `AUTH_REQUIRED`,
`UNSUPPORTED`, ...) and `error.retryable()` for whether a retry can help. See
[CORE.md](CORE.md) for the full `ErrorCode` list.

## Tests

`crates/sdk/tests/client.rs` exercises the mock provider end to end: the default
client registers it, `whoami`/`list_chats` answer from memory, chat search
filters, a sent message is visible on the next read, and an empty registry
reports `NotRegistered`.
