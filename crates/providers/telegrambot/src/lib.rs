//! Telegram Bot provider over the Bot API.
//!
//! The Bot API keeps no message history, so `list_messages` is a best-effort
//! reconstruction from `getUpdates`: it only sees updates the bot has not yet
//! confirmed and only for chats the bot can see, so it cannot page through past
//! messages the way the account-based providers do.

mod client;
mod models;

use std::collections::HashSet;

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatKind, ChatQuery,
    ConfigPaths, CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome,
    Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};

use client::TelegramBotClient;
use horon_messenger_provider_common::HttpClient;
use models::{BotUser, Message as TelegramMessage, TelegramChat};

const PROVIDER: ProviderKind = ProviderKind::TelegramBot;
const DEFAULT_ACCOUNT: &str = "default";
const TOKEN_ENV: &str = "HORON_MESSENGER_TELEGRAMBOT_TOKEN";

/// The Telegram Bot provider, authenticated with a bot token.
#[derive(Debug, Clone)]
pub struct TelegramBotProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    http: HttpClient,
}

impl TelegramBotProvider {
    /// Builds a provider rooted at `paths`.
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            http: HttpClient::shared(),
        }
    }

    /// The config paths this provider reads credentials from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The credential store backing the `default` account.
    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn token(&self) -> Result<Option<String>> {
        if let Ok(value) = std::env::var(TOKEN_ENV)
            && !value.is_empty()
        {
            return Ok(Some(value));
        }
        let stored = self.credentials.load(PROVIDER, DEFAULT_ACCOUNT)?;
        Ok(stored
            .and_then(|value| {
                value
                    .get("token")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|token| !token.is_empty()))
    }

    fn client(&self) -> Result<TelegramBotClient> {
        let token = self
            .token()?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        Ok(TelegramBotClient::new(self.http.clone(), token))
    }

    fn chat(chat: &TelegramChat) -> Chat {
        Chat {
            provider: PROVIDER,
            id: chat.id.to_string(),
            name: chat.display_name(),
            kind: chat_kind(&chat.kind),
            unread_count: None,
            last_message_at: None,
            member_count: chat.member_count,
            extra: Value::Null,
        }
    }

    fn message(message: &TelegramMessage) -> Message {
        Message {
            provider: PROVIDER,
            id: message.message_id.to_string(),
            chat_id: message.chat.id.to_string(),
            sender_id: message.from.as_ref().map(|user| user.id.to_string()),
            sender_name: message.from.as_ref().map(BotUser::display_name),
            text: message.body(),
            sent_at: unix_to_rfc3339(message.date),
            edited_at: None,
            attachments: Vec::new(),
            reply_to: message
                .reply_to_message
                .as_ref()
                .map(|reply| reply.message_id.to_string()),
            extra: Value::Null,
        }
    }
}

fn chat_kind(kind: &str) -> ChatKind {
    match kind {
        "private" => ChatKind::Direct,
        "group" | "supergroup" => ChatKind::Group,
        "channel" => ChatKind::Channel,
        _ => ChatKind::Unknown,
    }
}

fn unix_to_rfc3339(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(now_rfc3339)
}

#[async_trait]
impl Provider for TelegramBotProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        let user = self.client()?.get_me().await?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: user.id.to_string(),
            display_name: Some(user.display_name()),
            username: user.username.clone(),
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let updates = self.client()?.get_updates().await?;
        let mut seen = HashSet::new();
        let mut chats = Vec::new();
        for update in &updates {
            for chat in update.chats() {
                if seen.insert(chat.id) {
                    chats.push(Self::chat(chat));
                }
            }
        }
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| chat.name.to_lowercase().contains(&needle));
        }
        if let Some(limit) = query.limit {
            chats.truncate(limit);
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        let chat = self.client()?.get_chat(chat_id).await?;
        Ok(Self::chat(&chat))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let updates = self.client()?.get_updates().await?;
        let mut hits: Vec<&TelegramMessage> = updates
            .iter()
            .flat_map(|update| update.messages())
            .filter(|message| message.chat.id.to_string() == chat_id)
            .collect();
        hits.sort_by_key(|message| std::cmp::Reverse(message.message_id));
        let mut messages: Vec<Message> = hits.iter().copied().map(Self::message).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            messages.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        if let Some(limit) = query.limit {
            messages.truncate(limit);
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages,
            has_more: false,
            newest_first: true,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let message = self
            .client()?
            .send_message(&request.chat_id, &request.text, request.reply_to.as_deref())
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(message.message_id.to_string()),
            sent_at: unix_to_rfc3339(message.date),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let members = self.client()?.get_chat_administrators(chat_id).await?;
        Ok(members
            .into_iter()
            .map(|member| Member {
                id: member.user.id.to_string(),
                display_name: member.user.display_name(),
                username: member.user.username.clone(),
                is_self: false,
                extra: Value::Null,
            })
            .collect())
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
impl AuthProvider for TelegramBotProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Token]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let present = self.token()?.is_some();
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated: present,
            current: present.then(|| DEFAULT_ACCOUNT.to_owned()),
            accounts: if present {
                vec![AccountSummary {
                    account_id: DEFAULT_ACCOUNT.to_owned(),
                    display_name: None,
                    method: Some(AuthMethod::Token),
                    current: true,
                    extra: Value::Null,
                }]
            } else {
                Vec::new()
            },
        })
    }

    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome> {
        let token = options
            .params
            .get("token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| options.account_id.clone())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Telegram Bot login needs a token".to_owned(),
            })?;
        self.credentials.save(
            PROVIDER,
            DEFAULT_ACCOUNT,
            &serde_json::json!({ "token": token }),
        )?;
        Ok(LoginOutcome {
            provider: PROVIDER,
            authenticated: true,
            account_id: Some(DEFAULT_ACCOUNT.to_owned()),
            next_action: None,
            message: None,
        })
    }

    async fn logout(&self, _account_id: Option<&str>) -> Result<()> {
        self.credentials.delete(PROVIDER, DEFAULT_ACCOUNT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_kind_maps_bot_api_types() {
        assert_eq!(chat_kind("private"), ChatKind::Direct);
        assert_eq!(chat_kind("group"), ChatKind::Group);
        assert_eq!(chat_kind("supergroup"), ChatKind::Group);
        assert_eq!(chat_kind("channel"), ChatKind::Channel);
        assert_eq!(chat_kind("weird"), ChatKind::Unknown);
    }

    #[test]
    fn chat_mapping_prefers_title_then_name() {
        let group: TelegramChat =
            serde_json::from_value(json!({ "id": -100, "type": "supergroup", "title": "Team" }))
                .unwrap();
        let chat = TelegramBotProvider::chat(&group);
        assert_eq!(chat.id, "-100");
        assert_eq!(chat.name, "Team");
        assert_eq!(chat.kind, ChatKind::Group);

        let private: TelegramChat = serde_json::from_value(
            json!({ "id": 7, "type": "private", "first_name": "Ada", "last_name": "L" }),
        )
        .unwrap();
        assert_eq!(TelegramBotProvider::chat(&private).name, "Ada L");

        let username_only: TelegramChat =
            serde_json::from_value(json!({ "id": 8, "type": "private", "username": "ada" }))
                .unwrap();
        assert_eq!(TelegramBotProvider::chat(&username_only).name, "ada");
    }

    #[test]
    fn message_mapping_uses_text_and_reply() {
        let raw: TelegramMessage = serde_json::from_value(json!({
            "message_id": 9001,
            "date": 1_700_000_000,
            "chat": { "id": 5, "type": "private", "first_name": "Ada" },
            "from": { "id": 11, "is_bot": false, "first_name": "Ada", "username": "ada" },
            "text": "hello",
            "reply_to_message": { "message_id": 8999, "date": 1_699_999_000, "chat": { "id": 5, "type": "private" } }
        }))
        .unwrap();
        let message = TelegramBotProvider::message(&raw);
        assert_eq!(message.id, "9001");
        assert_eq!(message.chat_id, "5");
        assert_eq!(message.sender_id.as_deref(), Some("11"));
        assert_eq!(message.sender_name.as_deref(), Some("Ada"));
        assert_eq!(message.text, "hello");
        assert_eq!(message.reply_to.as_deref(), Some("8999"));
        assert!(
            message.sent_at.starts_with("2023-11-14T"),
            "{}",
            message.sent_at
        );
    }

    #[test]
    fn caption_is_used_when_text_is_absent() {
        let raw: TelegramMessage = serde_json::from_value(json!({
            "message_id": 1,
            "date": 0,
            "chat": { "id": 1, "type": "channel" },
            "caption": "photo caption"
        }))
        .unwrap();
        assert_eq!(TelegramBotProvider::message(&raw).text, "photo caption");
    }

    #[test]
    fn token_precedence_env_over_stored_over_absent() {
        let dir = std::env::temp_dir().join(format!("hm-telegrambot-test-{}", std::process::id()));
        let provider = TelegramBotProvider::new(ConfigPaths::at(dir.clone()));

        unsafe { std::env::remove_var(TOKEN_ENV) };
        assert!(provider.token().unwrap().is_none());

        provider
            .credentials()
            .save(PROVIDER, DEFAULT_ACCOUNT, &json!({ "token": "stored" }))
            .unwrap();
        assert_eq!(provider.token().unwrap().as_deref(), Some("stored"));

        unsafe { std::env::set_var(TOKEN_ENV, "from-env") };
        assert_eq!(provider.token().unwrap().as_deref(), Some("from-env"));

        unsafe { std::env::remove_var(TOKEN_ENV) };
        assert_eq!(provider.token().unwrap().as_deref(), Some("stored"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
