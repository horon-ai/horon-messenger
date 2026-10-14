//! Discord provider: a Rust port of the agent-messenger Discord client.

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

use client::DiscordClient;
use horon_messenger_provider_common::HttpClient;
use models::DiscordMessage;

const PROVIDER: ProviderKind = ProviderKind::Discord;
const DEFAULT_ACCOUNT: &str = "default";
const TOKEN_ENV: &str = "HORON_MESSENGER_DISCORD_TOKEN";
const E2E_TOKEN_ENV: &str = "E2E_DISCORD_TOKEN";

/// The Discord provider: token authentication over the REST v10 API.
#[derive(Debug, Clone)]
pub struct DiscordProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    http: HttpClient,
}

impl DiscordProvider {
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            http: HttpClient::shared(),
        }
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn token(&self) -> Result<Option<String>> {
        for key in [TOKEN_ENV, E2E_TOKEN_ENV] {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                return Ok(Some(value));
            }
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

    fn client(&self) -> Result<DiscordClient> {
        let token = self
            .token()?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        Ok(DiscordClient::new(self.http.clone(), token))
    }

    fn chat(guild: &models::DiscordGuild) -> Chat {
        Chat {
            provider: PROVIDER,
            id: guild.id.clone(),
            name: guild.name.clone(),
            kind: ChatKind::Channel,
            unread_count: None,
            last_message_at: None,
            member_count: None,
            extra: Value::Null,
        }
    }

    fn message(message: &DiscordMessage) -> Message {
        Message {
            provider: PROVIDER,
            id: message.id.clone(),
            chat_id: message.channel_id.clone(),
            sender_id: Some(message.author.id.clone()),
            sender_name: Some(message.author.username.clone()),
            text: message.content.clone(),
            sent_at: message.timestamp.clone(),
            edited_at: message.edited_timestamp.clone(),
            attachments: Vec::new(),
            reply_to: None,
            extra: Value::Null,
        }
    }
}

#[async_trait]
impl Provider for DiscordProvider {
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
            | Capabilities::LIST_MEMBERS
            | Capabilities::MARK_READ
            | Capabilities::SNAPSHOT
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        let user = self.client()?.test_auth().await?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: user.id,
            display_name: user.global_name.or(Some(user.username.clone())),
            username: Some(user.username),
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let mut chats: Vec<Chat> = self
            .client()?
            .list_guilds()
            .await?
            .iter()
            .map(Self::chat)
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
        let guild = self.client()?.get_guild(chat_id).await?;
        Ok(Self::chat(&guild))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let limit = query.limit.unwrap_or(50);
        let messages = self.client()?.list_messages(chat_id, limit).await?;
        let has_more = messages.len() >= limit;
        let mut mapped: Vec<Message> = messages.iter().map(Self::message).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            mapped.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages: mapped,
            has_more,
            newest_first: true,
        })
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        let guild_id = chat_id.ok_or_else(|| Error::InvalidInput {
            message: "Discord search needs a guild id".to_owned(),
        })?;
        let term = query.search.clone().unwrap_or_default();
        let response = self.client()?.search_messages(guild_id, &term).await?;
        let messages = response
            .messages
            .into_iter()
            .flatten()
            .filter(|hit| hit.hit)
            .map(|hit| Message {
                provider: PROVIDER,
                id: hit.id,
                chat_id: hit.channel_id,
                sender_id: Some(hit.author.id),
                sender_name: Some(hit.author.username),
                text: hit.content,
                sent_at: hit.timestamp,
                edited_at: None,
                attachments: Vec::new(),
                reply_to: None,
                extra: Value::Null,
            })
            .collect::<Vec<_>>();
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: guild_id.to_owned(),
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
        let sent_at = if message.timestamp.is_empty() {
            now_rfc3339()
        } else {
            message.timestamp.clone()
        };
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(message.id),
            sent_at,
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let members = self.client()?.list_members(chat_id).await?;
        Ok(members
            .into_iter()
            .map(|member| Member {
                id: member.user.id.clone(),
                display_name: member.nick.unwrap_or_else(|| member.user.username.clone()),
                username: Some(member.user.username),
                is_self: false,
                extra: Value::Null,
            })
            .collect())
    }

    async fn mark_read(&self, chat_id: &str) -> Result<()> {
        let client = self.client()?;
        let messages = client.list_messages(chat_id, 1).await?;
        let Some(newest) = messages.first() else {
            return Ok(());
        };
        client.ack_message(chat_id, &newest.id).await
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
impl AuthProvider for DiscordProvider {
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
                message: "Discord login needs a token".to_owned(),
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
