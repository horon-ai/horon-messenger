//! Channel Talk provider: a Rust port of the agent-messenger desk-api client.

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

use client::{ChannelTalkClient, ChatRef};
use horon_messenger_provider_common::HttpClient;
use models::ChannelMessage;

const PROVIDER: ProviderKind = ProviderKind::ChannelTalk;
const DEFAULT_ACCOUNT: &str = "default";
const ACCOUNT_ENV: &str = "HORON_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE";
const SESSION_ENV: &str = "HORON_MESSENGER_CHANNELTALK_SESSION_COOKIE";

#[derive(Debug, Clone)]
struct Credentials {
    account_cookie: String,
    session_cookie: Option<String>,
}

/// The Channel Talk provider: operator cookie authentication over the desk API.
#[derive(Debug, Clone)]
pub struct ChannelTalkProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    http: HttpClient,
}

impl ChannelTalkProvider {
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

    fn resolve(&self) -> Result<Option<Credentials>> {
        if let Ok(account_cookie) = std::env::var(ACCOUNT_ENV)
            && !account_cookie.is_empty()
        {
            return Ok(Some(Credentials {
                account_cookie,
                session_cookie: std::env::var(SESSION_ENV).ok().filter(|s| !s.is_empty()),
            }));
        }
        let stored = self.credentials.load(PROVIDER, DEFAULT_ACCOUNT)?;
        let account_cookie = stored
            .as_ref()
            .and_then(|value| value.get("account_cookie"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        Ok(account_cookie.map(|account_cookie| Credentials {
            account_cookie,
            session_cookie: stored
                .as_ref()
                .and_then(|value| value.get("session_cookie"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        }))
    }

    fn client(&self) -> Result<ChannelTalkClient> {
        let credentials = self
            .resolve()?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        Ok(ChannelTalkClient::new(
            self.http.clone(),
            credentials.account_cookie,
            credentials.session_cookie,
        ))
    }

    fn chat(channel: &models::Channel) -> Chat {
        Chat {
            provider: PROVIDER,
            id: channel.id.clone(),
            name: channel.name.clone(),
            kind: ChatKind::Channel,
            unread_count: None,
            last_message_at: None,
            member_count: None,
            extra: Value::Null,
        }
    }

    fn message(chat_id: &str, message: &ChannelMessage) -> Message {
        Message {
            provider: PROVIDER,
            id: message.id.clone(),
            chat_id: chat_id.to_owned(),
            sender_id: message.person_id.clone(),
            sender_name: message.person_type.clone(),
            text: message.text(),
            sent_at: ms_to_rfc3339(message.created_at),
            edited_at: None,
            attachments: Vec::new(),
            reply_to: None,
            extra: Value::Null,
        }
    }
}

fn ms_to_rfc3339(ms: Option<i64>) -> String {
    ms.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(now_rfc3339)
}

#[async_trait]
impl Provider for ChannelTalkProvider {
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
            | Capabilities::SNAPSHOT
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        let account = self.client()?.get_account().await?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: account.id,
            display_name: Some(account.name),
            username: Some(account.email),
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let mut chats: Vec<Chat> = self
            .client()?
            .list_channels()
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
        let channel = self.client()?.get_channel(chat_id).await?;
        Ok(Self::chat(&channel))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let reference = ChatRef::parse(chat_id)?;
        let limit = query.limit.unwrap_or(25);
        let messages = self.client()?.list_messages(&reference, limit).await?;
        let has_more = messages.len() >= limit;
        let mut mapped: Vec<Message> = messages
            .iter()
            .map(|message| Self::message(chat_id, message))
            .collect();
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
        let channel_id = chat_id.ok_or_else(|| Error::InvalidInput {
            message: "Channel Talk search needs a workspace id".to_owned(),
        })?;
        let term = query.search.clone().unwrap_or_default();
        let hits = self.client()?.search_messages(channel_id, &term).await?;
        let messages = hits
            .iter()
            .map(|hit| Self::message(channel_id, &hit.source))
            .collect();
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: channel_id.to_owned(),
            messages,
            has_more: false,
            newest_first: true,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let reference = ChatRef::parse(&request.chat_id)?;
        let message = self
            .client()?
            .send_message(&reference, &request.text)
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(message.id),
            sent_at: ms_to_rfc3339(message.created_at),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let managers = self.client()?.list_managers(chat_id).await?;
        Ok(managers
            .into_iter()
            .map(|manager| Member {
                id: manager.id,
                display_name: manager.name,
                username: manager.email,
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
impl AuthProvider for ChannelTalkProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Extract]
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
                    method: Some(AuthMethod::Extract),
                    current: true,
                    extra: Value::Null,
                }]
            } else {
                Vec::new()
            },
        })
    }

    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome> {
        let account_cookie = options
            .params
            .get("account_cookie")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Channel Talk login needs an account_cookie".to_owned(),
            })?;
        let mut value = serde_json::json!({ "account_cookie": account_cookie });
        if let Some(session) = options.params.get("session_cookie").and_then(Value::as_str) {
            value["session_cookie"] = Value::String(session.to_owned());
        }
        if let Some(workspace) = options.params.get("workspace_id").and_then(Value::as_str) {
            value["workspace_id"] = Value::String(workspace.to_owned());
        }
        self.credentials.save(PROVIDER, DEFAULT_ACCOUNT, &value)?;
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
