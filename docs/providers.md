# Providers

A provider is one crate under `crates/providers/<name>/` that implements the
`Provider` trait from `hyper-messenger-core`.

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
2. Add a `hyper-messenger-provider-<name>` entry to `[workspace.dependencies]`.
3. Add an optional dependency and a feature in `crates/sdk/Cargo.toml`
   (`<name> = ["dep:hyper-messenger-provider-<name>"]`), and include it in `all`.
4. Register it in `crates/sdk/src/lib.rs::register_builtin` behind
   `#[cfg(feature = "<name>")]`.
5. Add a `ProviderKind` variant if the platform is new.

The provider crate is always a workspace member, so `cargo build --workspace`
compiles it even when `sdk` does not re-export it.

## Platform notes

| Provider | Auth | Transport |
|----------|------|-----------|
| KakaoTalk | desktop session + derived DB key; tablet LOCO session for send | local SQLCipher DB read, LOCO over TLS for send, macOS Accessibility for UI-only actions |
| Discord | user token extracted from the desktop app or browser, or QR sign-in; bot token for server use | REST + gateway websocket for `listen` |
| Telegram | phone-code sign-in via TDLib; bot token for server use | TDLib (or Bot API) |
| Channel Talk | operator cookies, or API credentials | REST + websocket |

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
