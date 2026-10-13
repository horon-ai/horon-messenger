# Providers

A provider is one crate under `crates/providers/<name>/` that implements the
`Provider` trait from `horon-messenger-core`.

## Skeleton vs implementation

A new provider starts as a skeleton: a struct holding `ConfigPaths` and a
`FileCredentialStore`, plus `kind()` and `capabilities()`. It overrides no
operation, so every advertised capability reports `NotImplemented` and every
unadvertised one reports `Unsupported` through the trait's `gate`. That is a
truthful placeholder, not a silent no-op: calling it fails with a typed error
naming the provider and the operation.

To implement an operation, override it and delete nothing else:

```rust
#[async_trait]
impl Provider for DiscordProvider {
    fn kind(&self) -> ProviderKind { ProviderKind::Discord }
    fn capabilities(&self) -> Capabilities { /* ... */ }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let token = self.load_token().await?;
        // ...
    }
}
```

Implement only what `capabilities()` advertises. If an operation is genuinely out
of scope for the platform, leave it unadvertised so it reports `Unsupported`.

## Registering a provider

1. Add the crate under `crates/providers/<name>/` and to `members` in the root
   `Cargo.toml`.
2. Add a `horon-messenger-provider-<name>` entry to `[workspace.dependencies]`.
3. Add an optional dependency and a feature in `crates/sdk/Cargo.toml`
   (`<name> = ["dep:horon-messenger-provider-<name>"]`), and include it in `all`.
4. Register it in `crates/sdk/src/lib.rs::register_builtin` behind
   `#[cfg(feature = "<name>")]`.
5. Add a `ProviderKind` variant if the platform is new.

The provider crate is always a workspace member, so `cargo build --workspace`
compiles it even when `sdk` does not re-export it.

## Platform notes

Every provider below is a workspace member, so `cargo build --workspace` compiles
it; `cargo build --workspace --all-features` wires all of them into `sdk`.

| Provider | Auth | Transport | Prerequisites |
|----------|------|-----------|---------------|
| Mock (`mock`) | none | in-memory | none |
| KakaoTalk (`kakaotalk`, skeleton) | desktop session and a derived DB key; tablet LOCO session for send (planned) | local SQLCipher DB read, LOCO over TLS for send, macOS Accessibility for UI-only actions (planned) | macOS; operations are not implemented yet |
| Discord (`discord`) | user token from `HORON_MESSENGER_DISCORD_TOKEN` or `auth login --param token=...` | HTTP REST, base override `HORON_MESSENGER_DISCORD_API_BASE` | none |
| Channel Talk (`channeltalk`) | operator cookies (`account_cookie`, optional `session_cookie`) via `auth login --param` or `HORON_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE` / `_SESSION_COOKIE` | HTTP REST, base override `HORON_MESSENGER_CHANNELTALK_API_BASE` | none |
| Telegram Bot (`telegrambot`) | bot token from `HORON_MESSENGER_TELEGRAMBOT_TOKEN` or `auth login --param token=...` | HTTP, base override `HORON_MESSENGER_TELEGRAMBOT_API_BASE` | none |
| WhatsApp Bot (`whatsappbot`) | phone number id and access token from `HORON_MESSENGER_WHATSAPPBOT_PHONE_NUMBER_ID` / `_ACCESS_TOKEN` or `auth login --param` | HTTP, base override `HORON_MESSENGER_WHATSAPPBOT_API_BASE` | none |
| Slack Bot (`slackbot`) | bot token from `HORON_MESSENGER_SLACKBOT_TOKEN` or `auth login --param token=...` | HTTP, base override `HORON_MESSENGER_SLACKBOT_API_BASE` | none |
| Slack (`slack`) | user token and cookie from `HORON_MESSENGER_SLACK_TOKEN` / `_COOKIE` or `auth login --param token=... --param cookie=...` | HTTP, base override `HORON_MESSENGER_SLACK_API_BASE` | none |
| iMessage (`imessage`) | none; reads the local Messages app | sidecar over JSON-RPC (spawns `imsg`) | macOS, the `imsg` binary (`HORON_MESSENGER_IMSG_BIN`) and Full Disk Access |
| Telegram (`telegram`) | phone-code sign-in via TDLib (`api_id`, `api_hash`, phone) | TDLib native `libtdjson`, loaded at run time | `libtdjson` on the library path or `TDLIB_PATH` |
| WhatsApp (`whatsapp`) | device pairing (QR or pairing code) through the sidecar | Node sidecar over JSON-RPC (`@whiskeysockets/baileys`) | Node 20+, `npm ci` in `crates/providers/whatsapp/sidecar/`, env `HORON_MESSENGER_WHATSAPP_SIDECAR` and `HORON_MESSENGER_NODE_BIN` |
| LINE (`line`) | token or email and password sign-in through the sidecar | Node sidecar over JSON-RPC (linejs) | Node 20+, `npm install` in `crates/providers/line/sidecar/`, env `HORON_MESSENGER_LINE_SIDECAR` |
| Instagram (`instagram`) | session cookie (`sessionid`, `ds_user_id`) via `auth login --param` or `HORON_MESSENGER_INSTAGRAM_SESSIONID` / `_DS_USER_ID` | HTTP with a session `Cookie` header, base override `HORON_MESSENGER_INSTAGRAM_API_BASE` | none |

Every HTTP provider takes `HORON_MESSENGER_<NAME>_API_BASE` to point at a
different API base (tests, proxies). WhatsApp Bot advertises only `whoami` and
`send_message`.

Realtime listeners and desktop-app credential extraction are out of scope for
this expansion, so no provider above claims either. A provider reads credentials
you supply or runs its own sign-in flow.

## KakaoTalk port

The KakaoTalk adapter is ported from `~/Desktop/dev/alpox/kakaotalk-bridge`. That
project is single-platform and macos-only; here it becomes one provider among
many. What moves:

- The SQLCipher DB read path (auth, queries) and the native LOCO client
  (packets, crypto, session, tablet auth, media upload, replies).
- The macOS Accessibility send path and the input curtain, behind the same
  `AxAccess` seam the bridge already tests against.

What changes on the way in:

- Platform-specific macOS dependencies move behind `[target.'cfg(target_os = "macos")'.dependencies]`,
  and the headless core stays portable.
- The bridge's own JSON envelopes are replaced by `core::Envelope`.
- Credentials move under `<config>/kakaotalk/accounts/` through
  `FileCredentialStore`, keeping the owner-only, atomic-write guarantees.
