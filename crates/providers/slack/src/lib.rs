//! Slack provider: a user-token Web API client over `https://slack.com/api`.

mod client;
mod mappers;
mod models;

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatQuery,
    ConfigPaths, CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome,
    Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};

use horon_messenger_provider_common::HttpClient;

use client::SlackClient;

const PROVIDER: ProviderKind = ProviderKind::Slack;
const DEFAULT_ACCOUNT: &str = "default";
const TOKEN_ENV: &str = "HORON_MESSENGER_SLACK_TOKEN";
const COOKIE_ENV: &str = "HORON_MESSENGER_SLACK_COOKIE";

/// The Slack provider, authenticated with a user token plus `d=` cookie.
#[derive(Clone)]
pub struct SlackProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    http: HttpClient,
}

impl std::fmt::Debug for SlackProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackProvider")
            .field("paths", &self.paths)
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl SlackProvider {
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

    fn env(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    fn credentials_pair(&self) -> Result<(Option<String>, Option<String>)> {
        let stored = self.credentials.load(PROVIDER, DEFAULT_ACCOUNT)?;
        let stored_str = |key: &str| {
            stored
                .as_ref()
                .and_then(|value| value.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let token = Self::env(TOKEN_ENV)
            .or_else(|| stored_str("token"))
            .filter(|value| !value.is_empty());
        let cookie = Self::env(COOKIE_ENV)
            .or_else(|| stored_str("cookie"))
            .filter(|value| !value.is_empty());
        Ok((token, cookie))
    }

    fn client(&self) -> Result<SlackClient> {
        let (token, cookie) = self.credentials_pair()?;
        let token = token.ok_or(Error::AuthRequired { provider: PROVIDER })?;
        SlackClient::new(self.http.clone(), token, cookie)
    }
}

#[async_trait]
impl Provider for SlackProvider {
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
        let auth = self.client()?.auth_test().await?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: auth.user_id,
            display_name: auth.user.clone(),
            username: auth.user,
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let mut chats: Vec<Chat> = self
            .client()?
            .conversations_list()
            .await?
            .iter()
            .map(mappers::chat)
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
        let channel = self.client()?.conversations_info(chat_id).await?;
        Ok(mappers::chat(&channel))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let limit = query.limit.unwrap_or(50);
        let history = self
            .client()?
            .conversations_history(
                chat_id,
                limit,
                query.after.as_deref(),
                query.before.as_deref(),
            )
            .await?;
        let mut messages: Vec<Message> = history
            .messages
            .iter()
            .map(|message| mappers::message(chat_id, message))
            .collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            messages.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages,
            has_more: history.has_more,
            newest_first: true,
        })
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        let term = query.search.clone().unwrap_or_default();
        let response = self
            .client()?
            .search_messages(&term, query.limit.unwrap_or(20))
            .await?;
        Ok(mappers::search_page(chat_id.unwrap_or_default(), &response))
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let posted = self
            .client()?
            .chat_post_message(&request.chat_id, &request.text, request.reply_to.as_deref())
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: Some(posted.ts),
            sent_at: now_rfc3339(),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let client = self.client()?;
        let ids = client.conversations_members(chat_id).await?;
        let mut members = Vec::with_capacity(ids.len());
        for id in ids {
            let user = client.users_info(&id).await?;
            members.push(mappers::member(&user));
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
impl AuthProvider for SlackProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Token]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let (token, _) = self.credentials_pair()?;
        let present = token.is_some();
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
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Slack login needs a token".to_owned(),
            })?;
        let cookie = options
            .params
            .get("cookie")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Slack login needs a cookie".to_owned(),
            })?;
        self.credentials.save(
            PROVIDER,
            DEFAULT_ACCOUNT,
            &serde_json::json!({ "token": token, "cookie": cookie }),
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
