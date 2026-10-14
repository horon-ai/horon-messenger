# Core

`horon-messenger-core` is the contract layer every provider and surface agrees
on. It owns no platform logic: it defines the [`Provider`](#provider-contract)
trait, the domain model, capabilities, config and credential paths, the auth
contract, the error types, and the [`Envelope`](#wire-envelope) wire shape.

- Crate: `crates/core` (`horon-messenger-core`)
- Depended on by: every provider crate and `horon-messenger-sdk`

## Provider contract

`Provider` is the operation set. Each method has a default that reports why it is
unavailable, so a provider overrides only what it implements:

```rust
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;
    fn auth(&self) -> Option<&dyn AuthProvider> { None }

    async fn whoami(&self) -> Result<Profile>;
    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>>;
    async fn get_chat(&self, chat_id: &str) -> Result<Chat>;
    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage>;
    async fn search_messages(&self, chat_id: Option<&str>, query: &MessageQuery) -> Result<MessagePage>;
    async fn send_message(&self, request: &SendRequest) -> Result<SendResult>;
    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>>;
    async fn mark_read(&self, chat_id: &str) -> Result<()>;
    async fn snapshot(&self) -> Result<Snapshot>;
    async fn listen(&self, chat_id: Option<&str>) -> Result<EventStream>;
}
```

`DynProvider` is `Arc<dyn Provider>`. A default body calls `gate(capability)`:

```rust
async fn list_chats(&self, _query: &ChatQuery) -> Result<Vec<Chat>> {
    Err(self.gate(Capabilities::LIST_CHATS))
}
```

`gate` returns `NotImplemented` when the provider advertises the capability and
`Unsupported` when it does not. That one rule lets a skeleton provider report the
truth for every operation without a stub body, and lets a caller tell "not built
yet" from "never supported". Implement only what `capabilities()` advertises; an
operation a provider leaves unadvertised reports `Unsupported`.

## Capabilities

`Capabilities` is a bit set a provider advertises up front. `capabilities().names()`
returns the human names; `contains(flag)` tests one.

`whoami`, `list_chats`, `get_chat`, `list_messages`, `search_messages`,
`send_message`, `send_media`, `mark_read`, `list_members`, `snapshot`, `listen`.

## Domain model

| Type | Fields (abridged) |
|------|-------------------|
| `Profile` | `provider`, `account_id`, `display_name?`, `username?`, `extra` |
| `Chat` | `provider`, `id`, `name`, `kind`, `unread_count?`, `last_message_at?`, `member_count?`, `extra` |
| `ChatKind` | `direct`, `group`, `channel`, `open_chat`, `thread`, `unknown` |
| `Member` | `id`, `display_name`, `username?`, `is_self`, `extra` |
| `Message` | `provider`, `id`, `chat_id`, `sender_id?`, `sender_name?`, `text`, `sent_at`, `edited_at?`, `attachments`, `reply_to?`, `extra` |
| `Attachment` | `id?`, `file_name?`, `mime_type?`, `size_bytes?`, `url?`, `local_path?` |
| `MessagePage` | `provider`, `chat_id`, `messages`, `has_more`, `newest_first` |
| `SendRequest` | `chat_id`, `text`, `reply_to?`, `attachments`, `idempotency_key?` |
| `SendResult` | `provider`, `chat_id`, `message_id?`, `sent_at`, `accepted`, `delivery`, `extra` |
| `Delivery` | `unverified`, `server_accepted`, `confirmed` |
| `Snapshot` | `provider`, `profile?`, `chats`, `generated_at` |
| `Event` | `provider`, `chat_id`, `kind`, `at`, `message?`, `extra` |
| `EventKind` | `message`, `read`, `typing`, `unknown` |
| `EventStream` | `Pin<Box<dyn Stream<Item = Result<Event>> + Send>>`, returned by `listen` |

`ChatQuery` is `{ search?, limit? }`; `MessageQuery` adds `before?` and `after?`.
A provider-specific field goes in the `extra` value rather than widening the
shared model. Identifiers that cross a boundary are strings, so a platform's
64-bit or snowflake id survives JSON without precision loss.

## Auth

Sign-in is separate from messaging so a provider can read and send while
delegating sign-in to a CLI-only flow. A provider that supports it implements
`AuthProvider` and returns `Some(self)` from `Provider::auth()`.

```rust
#[async_trait]
pub trait AuthProvider {
    fn kind(&self) -> ProviderKind;
    fn methods(&self) -> &'static [AuthMethod];   // extract, login, qr, token, bot_token
    async fn status(&self) -> Result<AuthStatus>;
    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome>;
    async fn logout(&self, account_id: Option<&str>) -> Result<()>;
}
```

`AuthStatus` carries `authenticated`, the current account, and `AccountSummary`
entries. `LoginOptions.params` is a provider-defined JSON object (the CLI fills
it from `--param key=value`).

## Wire envelope

Every surface returns the same shape.

```json
{"ok": true,  "data": { }, "meta": {"provider": "mock", "generated_at": "..."}}
{"ok": false, "error": {"code": "NOT_IMPLEMENTED", "message": "...", "retryable": false, "provider": "kakaotalk"}, "meta": { }}
```

`Envelope::success`, `Envelope::failure`, and `Envelope::from_result` build it;
`to_json` / `to_pretty` render it. `Meta` carries the optional provider, the
timestamp, and any warnings.

`ErrorCode` is a stable enum: `UNSUPPORTED`, `NOT_IMPLEMENTED`, `NOT_REGISTERED`,
`AUTH_REQUIRED`, `AUTH_FAILED`, `NOT_FOUND`, `AMBIGUOUS`, `RATE_LIMITED`,
`NETWORK`, `PROTOCOL`, `INVALID_INPUT`, `IO`, `CONFIG`, `INTERNAL`.
`Error::retryable()` is true only for the transient classes (`RATE_LIMITED`,
`NETWORK`, `IO`). `Error::provider()` returns the platform that produced it, when
one did.

## Config and credentials

```
<config>/<provider>/accounts/<account>.json   # one file per account, 0600
<config>/<provider>/<name>.json               # provider state (cursors, caches)
```

`<config>` is `HORON_MESSENGER_CONFIG_DIR`, else the platform config dir plus
`horon-messenger` (`ConfigPaths::resolve`). Directories are created `0700`;
files are written atomically through `write_private_atomic`, so a reader never
sees a partial file. Providers choose their own on-disk schema; core owns only
the location and the safe write. `FileCredentialStore` reads and writes the
per-account files.

## Registry

`ProviderRegistry` is the set of adapters a process compiled in, keyed by
`ProviderKind`. `get` / `require` resolve one (`require` returns `NotRegistered`
when absent); `kinds`, `iter`, `len` enumerate it. `Client` wraps a registry
plus the resolved config paths.

## Timestamps

`now_rfc3339()` produces RFC 3339 in UTC with milliseconds, so every surface
stamps timestamps identically and they cannot drift.

## Status

The contract is complete and stable: all types, capabilities, errors, config and
the envelope above are implemented. `listen` returns an `EventStream` a provider
builds (`Event`, `EventKind`); the mock provider implements it, and a provider
that does not advertise `LISTEN` reports `Unsupported` through `gate`.
`send_media` is declared on the trait but no provider implements it yet, so it
reports `NotImplemented`/`Unsupported` — the honest answer for both. Realtime
listening is per-provider: the mock streams its seeded and sent messages; a
platform provider implements `listen` when it can.
