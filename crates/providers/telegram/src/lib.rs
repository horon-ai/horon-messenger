//! Telegram provider for `hyper-messenger`.
//!
//! Backed by TDLib through the `tdjson` JSON API: chats and messages map from
//! TDLib JSON to the core `Chat`/`Message` model, and auth follows the TDLib
//! authorization state with phone, code and password passed through
//! `LoginOptions.params`. The real backend is gated behind the non-default
//! `tdlib` feature, so the workspace default build stays clean and needs no
//! native library.
//!
//! `libtdjson` is loaded at runtime (see [`tdlib_library`]) instead of being
//! linked at build time. The `tdlib` feature depends on `tdlib-rs` WITHOUT its
//! `download-tdlib` helper: that helper downloads and links a native `libtdjson`
//! during the build, which `cargo build --workspace --all-features` must not
//! require, and `tdlib-rs` binds its FFI at link time with no runtime-loading
//! API. The provider therefore resolves the three `tdjson` entry points through
//! `libloading`, matching the reference bridge, and the real backend stays
//! available by providing `libtdjson` at runtime (`TDLIB_PATH` or the library
//! search path). When the library cannot be loaded, or no backend is compiled
//! in, every operation returns a typed `Error::Config` and never panics. The
//! `qa-fake` feature swaps in an in-crate fake that replays fixture TDLib JSON
//! through the same mapping functions, so tests and QA never need `libtdjson`.

use std::sync::Mutex;

use async_trait::async_trait;
use hyper_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatQuery,
    ConfigPaths, CredentialStore, Delivery, FileCredentialStore, LoginOptions, LoginOutcome,
    Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};

use crate::transport::TelegramBackend;

pub mod mapping;
#[cfg(feature = "qa-fake")]
pub mod qa_fake;
#[cfg(feature = "tdlib")]
pub mod tdlib;
pub mod tdlib_library;
pub mod transport;

const PROVIDER: ProviderKind = ProviderKind::Telegram;
const DEFAULT_ACCOUNT: &str = "default";
const DEFAULT_LIMIT: usize = 50;

/// The Telegram provider, backed by TDLib or the `qa-fake` fixture replay.
pub struct TelegramProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    account_id: Mutex<String>,
}

impl TelegramProvider {
    /// Builds a provider rooted at `paths`.
    pub fn new(paths: ConfigPaths) -> Self {
        let account_id = FileCredentialStore::new(paths.clone())
            .list(PROVIDER)
            .ok()
            .and_then(|mut accounts| {
                accounts.sort();
                accounts.into_iter().next()
            })
            .unwrap_or_else(|| DEFAULT_ACCOUNT.to_owned());
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            account_id: Mutex::new(account_id),
        }
    }

    /// The config paths this provider reads credentials from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The credential store backing the current account.
    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    /// The account id every operation is attributed to.
    pub fn account_id(&self) -> String {
        self.account_id
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| DEFAULT_ACCOUNT.to_owned())
    }

    fn set_account(&self, account_id: String) {
        if let Ok(mut guard) = self.account_id.lock() {
            *guard = account_id;
        }
    }

    fn backend(&self) -> Result<Box<dyn TelegramBackend>> {
        #[cfg(feature = "qa-fake")]
        {
            Ok(Box::new(crate::qa_fake::QaFakeBackend::new()))
        }
        #[cfg(all(not(feature = "qa-fake"), feature = "tdlib"))]
        {
            let account = self.account_id();
            let config = crate::tdlib::TdlibConfig::load(&self.paths, &account)?;
            Ok(Box::new(crate::tdlib::TdlibBackend::new(
                config,
                account,
                self.paths.clone(),
            )))
        }
        #[cfg(all(not(feature = "qa-fake"), not(feature = "tdlib")))]
        {
            let _ = &self.paths;
            Err(hyper_messenger_core::Error::Config {
                message: "the Telegram provider has no TDLib backend in this build; enable the \
                          `tdlib` feature (runtime libtdjson) or the `qa-fake` feature (fixtures)"
                    .to_owned(),
            })
        }
    }

    fn page(chat_id: &str, messages: Vec<Message>, newest_first: bool) -> MessagePage {
        MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages,
            has_more: false,
            newest_first,
        }
    }
}

/// The stored account id for a phone number, so `auth login --param phone=...` is stable.
pub fn account_id_for(phone: Option<&str>) -> String {
    let raw = phone.unwrap_or_default().trim();
    if raw.is_empty() {
        return DEFAULT_ACCOUNT.to_owned();
    }
    let had_plus = raw.starts_with('+');
    let normalized = raw
        .to_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let trimmed = normalized.trim_matches('-');
    if trimmed.is_empty() {
        return DEFAULT_ACCOUNT.to_owned();
    }
    if had_plus {
        format!("plus-{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

#[async_trait]
impl Provider for TelegramProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEARCH_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::SEND_MEDIA
            | Capabilities::MARK_READ
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
            | Capabilities::LISTEN
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        self.backend()?.whoami().await
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let mut chats = self
            .backend()?
            .list_chats(query.limit.unwrap_or(DEFAULT_LIMIT))
            .await?;
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| {
                chat.name.to_lowercase().contains(&needle)
                    || chat.id.to_lowercase().contains(&needle)
            });
        }
        if let Some(limit) = query.limit {
            chats.truncate(limit);
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        self.backend()?.get_chat(chat_id).await
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let mut messages = self
            .backend()?
            .list_messages(chat_id, query.limit.unwrap_or(DEFAULT_LIMIT))
            .await?;
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            messages.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        if let Some(limit) = query.limit {
            messages.truncate(limit);
        }
        Ok(Self::page(chat_id, messages, true))
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        let needle = query.search.clone().unwrap_or_default();
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        let messages = self
            .backend()?
            .search_messages(chat_id, &needle, limit)
            .await?;
        Ok(Self::page(chat_id.unwrap_or_default(), messages, true))
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let message_id = self
            .backend()?
            .send_message(&request.chat_id, &request.text, request.reply_to.as_deref())
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(message_id),
            sent_at: now_rfc3339(),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: serde_json::Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        self.backend()?.list_members(chat_id).await
    }

    async fn mark_read(&self, chat_id: &str) -> Result<()> {
        self.backend()?.mark_read(chat_id).await
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        let profile = self.whoami().await?;
        let chats = self.list_chats(&ChatQuery::default()).await?;
        Ok(Snapshot {
            provider: PROVIDER,
            profile: Some(profile),
            chats,
            generated_at: now_rfc3339(),
        })
    }
}

#[async_trait]
impl AuthProvider for TelegramProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Login, AuthMethod::Token]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let (_, authenticated, _) = self.backend()?.authorization().await?;
        let accounts = self.credentials.list(PROVIDER)?;
        let current = self.account_id();
        let summaries = accounts
            .into_iter()
            .map(|account_id| AccountSummary {
                current: account_id == current,
                account_id,
                display_name: None,
                method: Some(AuthMethod::Login),
                extra: serde_json::Value::Null,
            })
            .collect();
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated,
            current: Some(current),
            accounts: summaries,
        })
    }

    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome> {
        let account_id = options.account_id.clone().unwrap_or_else(|| {
            account_id_for(
                options
                    .params
                    .get("phone")
                    .and_then(serde_json::Value::as_str),
            )
        });
        let mut stored = self
            .credentials
            .load(PROVIDER, &account_id)?
            .unwrap_or_else(|| serde_json::json!({}));
        for key in ["api_id", "api_hash", "phone_number", "phone"] {
            if let Some(value) = options.params.get(key) {
                stored[key] = value.clone();
            }
        }
        if stored.get("phone_number").is_none()
            && let Some(phone) = options.params.get("phone")
        {
            stored["phone_number"] = phone.clone();
        }
        self.credentials.save(PROVIDER, &account_id, &stored)?;
        self.set_account(account_id.clone());

        let (state, authenticated, next_action) = self.backend()?.login(&options.params).await?;
        Ok(LoginOutcome {
            provider: PROVIDER,
            authenticated,
            account_id: Some(account_id),
            next_action,
            message: Some(state),
        })
    }

    async fn logout(&self, account_id: Option<&str>) -> Result<()> {
        let account_id = account_id
            .map(str::to_owned)
            .unwrap_or_else(|| self.account_id());
        self.credentials.delete(PROVIDER, &account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper_messenger_core::{ChatKind, Error};
    use serde_json::json;
    use std::path::PathBuf;
    #[cfg(feature = "qa-fake")]
    use tokio::sync::Mutex as AsyncMutex;

    #[cfg(feature = "qa-fake")]
    static ENV_LOCK: AsyncMutex<()> = AsyncMutex::const_new(());

    #[cfg(feature = "qa-fake")]
    async fn env_guard() -> tokio::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().await
    }

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
    }

    fn value(name: &str) -> serde_json::Value {
        mapping::parse_response(&fixture(name)).expect("fixture must parse")
    }

    #[test]
    fn profile_maps_display_name_and_username() {
        let profile = mapping::profile(&value("get_me.json")).unwrap();
        assert_eq!(profile.provider, ProviderKind::Telegram);
        assert_eq!(profile.account_id, "777000");
        assert_eq!(profile.display_name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(profile.username.as_deref(), Some("ada"));
    }

    #[test]
    fn chat_maps_private_group_and_channel_kinds() {
        let direct = mapping::chat(&value("chat_1001.json")).unwrap();
        assert_eq!(direct.id, "1001");
        assert_eq!(direct.name, "Ada Lovelace");
        assert_eq!(direct.kind, ChatKind::Direct);
        assert_eq!(direct.unread_count, Some(3));
        assert!(
            direct
                .last_message_at
                .as_deref()
                .unwrap()
                .starts_with("2023-11-14T")
        );

        let group = mapping::chat(&value("chat_1002.json")).unwrap();
        assert_eq!(group.kind, ChatKind::Group);
        assert_eq!(group.member_count, Some(5));

        let channel = mapping::chat(&json!({
            "id": 9,
            "title": "News",
            "type": { "@type": "chatTypeSupergroup", "is_channel": true }
        }))
        .unwrap();
        assert_eq!(channel.kind, ChatKind::Channel);
    }

    #[test]
    fn chat_ids_are_read_from_a_chats_object() {
        let ids = mapping::chat_ids(&value("chats.json")).unwrap();
        assert_eq!(ids, vec![1001, 1002]);
        assert!(mapping::chat_ids(&json!({ "@type": "chats" })).is_err());
    }

    #[test]
    fn message_maps_text_reply_edit_and_photo_attachment() {
        let history = value("history_1001.json");
        let messages = history.get("messages").unwrap().as_array().unwrap();

        let photo = mapping::message(&messages[0], 1001).unwrap();
        assert_eq!(photo.id, "502");
        assert_eq!(photo.chat_id, "1001");
        assert_eq!(photo.text, "look at this");
        assert_eq!(photo.reply_to.as_deref(), Some("500"));
        assert!(photo.edited_at.is_some());
        assert_eq!(photo.attachments.len(), 1);
        assert_eq!(photo.attachments[0].id.as_deref(), Some("900"));
        assert_eq!(
            photo.attachments[0].local_path.as_deref(),
            Some("/tmp/photo.jpg")
        );

        let text = mapping::message(&messages[1], 1001).unwrap();
        assert_eq!(text.text, "see you soon");
        assert_eq!(text.sender_id.as_deref(), Some("777000"));
        assert!(text.attachments.is_empty());
    }

    #[test]
    fn message_sender_chat_id_is_used_when_present() {
        let message = mapping::message(
            &json!({
                "id": 5,
                "chat_id": 1002,
                "date": 1700000000,
                "sender_id": { "@type": "messageSenderChat", "chat_id": 999 },
                "content": { "@type": "messageText", "text": { "@type": "formattedText", "text": "hi" } }
            }),
            0,
        )
        .unwrap();
        assert_eq!(message.chat_id, "1002");
        assert_eq!(message.sender_id.as_deref(), Some("999"));
    }

    #[test]
    fn member_marks_self_and_reads_usernames() {
        let members = value("members_1002.json");
        let list = members.get("members").unwrap().as_array().unwrap();
        let ada = mapping::member(list[0].get("user").unwrap(), Some(777000)).unwrap();
        assert_eq!(ada.id, "777000");
        assert_eq!(ada.display_name, "Ada Lovelace");
        assert_eq!(ada.username.as_deref(), Some("ada"));
        assert!(ada.is_self);

        let grace = mapping::member(list[1].get("user").unwrap(), Some(777000)).unwrap();
        assert_eq!(grace.display_name, "Grace Hopper");
        assert!(!grace.is_self);
    }

    #[test]
    fn authorization_state_summarizes_the_flow() {
        let ready = mapping::authorization_state(&value("auth_state_ready.json"));
        assert!(ready.authenticated);
        assert_eq!(ready.next_action, None);

        let code = mapping::authorization_state(&json!({ "@type": "authorizationStateWaitCode" }));
        assert!(!code.authenticated);
        assert_eq!(code.next_action.as_deref(), Some("provide_code"));

        let closed = mapping::authorization_state(&json!({ "@type": "authorizationStateClosed" }));
        assert!(!closed.authenticated);
        assert_eq!(closed.next_action, None);
    }

    #[test]
    fn malformed_json_is_a_protocol_error() {
        let error = mapping::parse_response(&fixture("malformed.json")).unwrap_err();
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
        assert_eq!(error.code(), hyper_messenger_core::ErrorCode::Protocol);
    }

    #[test]
    fn a_message_missing_its_id_is_a_protocol_error() {
        let error = mapping::message(&json!({ "chat_id": 1 }), 1).unwrap_err();
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
    }

    #[test]
    fn missing_library_load_returns_a_typed_error_without_panicking() {
        let error =
            tdlib_library::TdlibLibrary::load_from(&[PathBuf::from("/no/such/libtdjson.dylib")])
                .err()
                .expect("a missing library must be an error");
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");

        let empty = tdlib_library::TdlibLibrary::load_from(&[])
            .err()
            .expect("an empty candidate list must be an error");
        assert!(matches!(empty, Error::Config { .. }), "got {empty:?}");
    }

    #[test]
    fn account_id_derives_from_a_phone_number() {
        assert_eq!(
            account_id_for(Some("+82 10-0000-0000")),
            "plus-82-10-0000-0000"
        );
        assert_eq!(account_id_for(Some("  ")), "default");
        assert_eq!(account_id_for(None), "default");
    }

    #[cfg(all(not(feature = "qa-fake"), not(feature = "tdlib")))]
    #[test]
    fn backend_absent_is_a_typed_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("hm-telegram-{}", std::process::id()));
        let provider = TelegramProvider::new(ConfigPaths::at(dir));
        let error = provider.backend().err().expect("no backend is compiled in");
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");
    }

    #[cfg(feature = "qa-fake")]
    #[tokio::test]
    async fn qa_fake_replays_fixtures_through_the_provider() {
        let _guard = env_guard().await;
        let dir = std::env::temp_dir().join(format!("hm-telegram-fake-{}", std::process::id()));
        let provider = TelegramProvider::new(ConfigPaths::at(dir));

        assert_eq!(provider.whoami().await.unwrap().account_id, "777000");

        let chats = provider.list_chats(&ChatQuery::default()).await.unwrap();
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].name, "Ada Lovelace");

        let page = provider
            .list_messages("1001", &MessageQuery::default())
            .await
            .unwrap();
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].text, "look at this");

        let found = provider
            .search_messages(
                None,
                &MessageQuery {
                    search: Some("standup".to_owned()),
                    ..MessageQuery::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(found.messages.len(), 1);
        assert_eq!(found.messages[0].text, "standup in 5");

        let members = provider.list_members("1002").await.unwrap();
        assert_eq!(members.len(), 2);
        assert!(members[0].is_self);

        let sent = provider
            .send_message(&SendRequest {
                chat_id: "1001".to_owned(),
                text: "hello".to_owned(),
                ..SendRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(sent.message_id.as_deref(), Some("9001"));
    }

    #[cfg(feature = "qa-fake")]
    #[tokio::test]
    async fn stale_closed_state_returns_auth_required() {
        let _guard = env_guard().await;
        unsafe { std::env::set_var("HYPER_MESSENGER_TELEGRAM_FAKE_STATE", "closed") };
        let dir = std::env::temp_dir().join(format!("hm-telegram-closed-{}", std::process::id()));
        let provider = TelegramProvider::new(ConfigPaths::at(dir));
        let error = provider.whoami().await.unwrap_err();
        unsafe { std::env::remove_var("HYPER_MESSENGER_TELEGRAM_FAKE_STATE") };
        assert!(matches!(error, Error::AuthRequired { .. }), "got {error:?}");
    }

    #[cfg(feature = "qa-fake")]
    #[tokio::test]
    async fn fake_malformed_fixture_is_a_protocol_error() {
        let _guard = env_guard().await;
        unsafe { std::env::set_var("HYPER_MESSENGER_TELEGRAM_FAKE_MALFORMED", "1") };
        let dir = std::env::temp_dir().join(format!("hm-telegram-bad-{}", std::process::id()));
        let provider = TelegramProvider::new(ConfigPaths::at(dir));
        let error = provider.whoami().await.unwrap_err();
        unsafe { std::env::remove_var("HYPER_MESSENGER_TELEGRAM_FAKE_MALFORMED") };
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
    }
}
