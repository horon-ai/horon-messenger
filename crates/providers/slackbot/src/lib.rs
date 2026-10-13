//! Slack Bot provider: a bot-token (`xoxb-`) client over the Slack Web API.
//!
//! A thin `reqwest` client is used instead of `slack-morphism` to keep the workspace dependency-light while hitting the same Slack Web API endpoints.

mod client;
mod models;

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatKind, ChatQuery,
    ConfigPaths, CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome,
    Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};

use client::SlackBotClient;

const PROVIDER: ProviderKind = ProviderKind::SlackBot;
const DEFAULT_ACCOUNT: &str = "default";
const TOKEN_ENV: &str = "HORON_MESSENGER_SLACKBOT_TOKEN";

/// The Slack Bot provider: bot-token authentication over the Slack Web API.
#[derive(Debug, Clone)]
pub struct SlackBotProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl SlackBotProvider {
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
        }
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn resolve(&self) -> Result<Option<String>> {
        let token = match std::env::var(TOKEN_ENV)
            .ok()
            .filter(|value| !value.is_empty())
        {
            Some(token) => Some(token),
            None => self
                .credentials
                .load(PROVIDER, DEFAULT_ACCOUNT)?
                .and_then(|value| {
                    value
                        .get("token")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .filter(|token| !token.is_empty()),
        };
        if let Some(token) = &token {
            validate_token(token)?;
        }
        Ok(token)
    }

    fn client(&self) -> Result<SlackBotClient> {
        let token = self
            .resolve()?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        SlackBotClient::new(token)
    }
}

/// Rejects any token that is not a Slack bot token (`xoxb-`).
pub fn validate_token(token: &str) -> Result<()> {
    if token.starts_with("xoxb-") {
        Ok(())
    } else {
        Err(Error::InvalidInput {
            message: "Slack bot token must start with `xoxb-`".to_owned(),
        })
    }
}

fn ts_to_rfc3339(ts: &str) -> String {
    let seconds: f64 = ts.parse().unwrap_or(0.0);
    chrono::DateTime::from_timestamp_millis((seconds * 1000.0) as i64)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(now_rfc3339)
}

fn chat(channel: &models::Channel) -> Chat {
    Chat {
        provider: PROVIDER,
        id: channel.id.clone(),
        name: channel.name.clone().unwrap_or_else(|| channel.id.clone()),
        kind: ChatKind::Channel,
        unread_count: None,
        last_message_at: None,
        member_count: channel.num_members,
        extra: Value::Null,
    }
}

#[async_trait]
impl Provider for SlackBotProvider {
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
        let info = self.client()?.test_auth().await?;
        let mut extra = serde_json::Map::new();
        extra.insert("team_id".to_owned(), Value::String(info.team_id.clone()));
        if let Some(bot_id) = &info.bot_id {
            extra.insert("bot_id".to_owned(), Value::String(bot_id.clone()));
        }
        Ok(Profile {
            provider: PROVIDER,
            account_id: info.user_id,
            display_name: info.team,
            username: info.user,
            extra: Value::Object(extra),
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let limit = query.limit.unwrap_or(200).max(1);
        let mut chats: Vec<Chat> = self
            .client()?
            .list_channels(limit)
            .await?
            .iter()
            .map(chat)
            .collect();
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
        let channel = self.client()?.get_channel(chat_id).await?;
        Ok(chat(&channel))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let limit = query.limit.unwrap_or(20).max(1);
        let history = self.client()?.history(chat_id, limit).await?;
        let messages: Vec<Message> = history
            .messages
            .iter()
            .map(|message| Message {
                provider: PROVIDER,
                id: message.ts.clone(),
                chat_id: chat_id.to_owned(),
                sender_id: message.user.clone(),
                sender_name: None,
                text: message.text.clone().unwrap_or_default(),
                sent_at: ts_to_rfc3339(&message.ts),
                edited_at: None,
                attachments: Vec::new(),
                reply_to: message.thread_ts.clone(),
                extra: Value::Null,
            })
            .collect();
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages,
            has_more: history.has_more,
            newest_first: true,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let posted = self
            .client()?
            .post_message(&request.chat_id, &request.text, request.reply_to.as_deref())
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(posted.ts.clone()),
            sent_at: ts_to_rfc3339(&posted.ts),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let client = self.client()?;
        let ids = client.members(chat_id).await?;
        let mut members = Vec::with_capacity(ids.len());
        for id in ids {
            let user = client.user_info(&id).await?.user;
            let display_name = user.display_name();
            members.push(Member {
                id: user.id.clone(),
                display_name,
                username: user.name.clone(),
                is_self: false,
                extra: Value::Null,
            });
        }
        Ok(members)
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
impl AuthProvider for SlackBotProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::BotToken]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let present = self.resolve()?.is_some();
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated: present,
            current: present.then(|| DEFAULT_ACCOUNT.to_owned()),
            accounts: if present {
                vec![AccountSummary {
                    account_id: DEFAULT_ACCOUNT.to_owned(),
                    display_name: None,
                    method: Some(AuthMethod::BotToken),
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
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Slack Bot login needs a token".to_owned(),
            })?;
        validate_token(&token)?;
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
    use horon_messenger_core::ErrorCode;

    #[test]
    fn bot_token_prefix_is_accepted() {
        assert!(validate_token("xoxb-123-abc").is_ok());
    }

    #[test]
    fn non_bot_tokens_are_invalid_input() {
        for token in ["xoxp-user", "xapp-app", "", "xoxb", "abc"] {
            let error = validate_token(token).unwrap_err();
            assert_eq!(error.code(), ErrorCode::InvalidInput, "for `{token}`");
        }
    }
}
