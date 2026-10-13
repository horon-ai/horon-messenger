//! Instagram provider: a mobile API client over `https://i.instagram.com/api/v1`.

mod client;
mod mappers;
mod models;
mod session;

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatQuery,
    ConfigPaths, CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome,
    Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};

use client::InstagramClient;
use models::Item;
use session::InstagramSession;

const PROVIDER: ProviderKind = ProviderKind::Instagram;
const DEFAULT_ACCOUNT: &str = "default";
const SESSIONID_ENV: &str = "HORON_MESSENGER_INSTAGRAM_SESSIONID";
const DS_USER_ID_ENV: &str = "HORON_MESSENGER_INSTAGRAM_DS_USER_ID";
const DEVICE_ID_ENV: &str = "HORON_MESSENGER_INSTAGRAM_DEVICE_ID";
const CSRFTOKEN_ENV: &str = "HORON_MESSENGER_INSTAGRAM_CSRFTOKEN";

/// The Instagram provider: account authentication over the Instagram mobile API.
#[derive(Debug, Clone)]
pub struct InstagramProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl InstagramProvider {
    /// Builds a provider rooted at `paths`.
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
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

    fn env(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    fn session(&self) -> Result<InstagramSession> {
        let stored = self.credentials.load(PROVIDER, DEFAULT_ACCOUNT)?;
        let stored_str = |key: &str| {
            stored
                .as_ref()
                .and_then(|value| value.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let sessionid = Self::env(SESSIONID_ENV)
            .or_else(|| stored_str("sessionid"))
            .filter(|value| !value.is_empty())
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        let ds_user_id = Self::env(DS_USER_ID_ENV)
            .or_else(|| stored_str("ds_user_id"))
            .filter(|value| !value.is_empty())
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        Ok(InstagramSession {
            sessionid,
            ds_user_id,
            device_id: Self::env(DEVICE_ID_ENV)
                .or_else(|| stored_str("device_id"))
                .filter(|value| !value.is_empty()),
            csrftoken: Self::env(CSRFTOKEN_ENV)
                .or_else(|| stored_str("csrftoken"))
                .filter(|value| !value.is_empty()),
        })
    }

    fn client(&self) -> Result<InstagramClient> {
        InstagramClient::new(self.session()?)
    }
}

#[async_trait]
impl Provider for InstagramProvider {
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
        let response = self.client()?.current_user().await?;
        let user = response.user.ok_or_else(|| Error::Protocol {
            provider: PROVIDER,
            message: "current_user response is missing the user".to_owned(),
        })?;
        Ok(mappers::profile(&user))
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let fetch = query.limit.unwrap_or(20).max(20);
        let threads = self.client()?.inbox(fetch).await?;
        let mut chats: Vec<Chat> = threads.iter().map(mappers::chat).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| {
                chat.name.to_lowercase().contains(&needle) || chat.id.contains(search.as_str())
            });
        }
        if let Some(limit) = query.limit {
            chats.truncate(limit);
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        let thread = self.client()?.thread(chat_id, 1).await?;
        Ok(mappers::chat(&thread))
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let limit = query.limit.unwrap_or(25);
        let thread = self.client()?.thread(chat_id, limit).await?;
        let mut items: Vec<&Item> = thread.items.iter().collect();
        items.reverse();
        let mut messages: Vec<Message> = items
            .into_iter()
            .map(|item| mappers::message(item, chat_id))
            .collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            messages.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages,
            has_more: false,
            newest_first: false,
        })
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        let term = query.search.clone().unwrap_or_default().to_lowercase();
        let limit = query.limit.unwrap_or(20);
        let client = self.client()?;
        let thread_ids: Vec<String> = match chat_id {
            Some(id) => vec![id.to_owned()],
            None => client
                .inbox(limit.max(20))
                .await?
                .iter()
                .filter_map(|thread| thread.thread_id.clone())
                .filter(|id| !id.is_empty())
                .collect(),
        };
        let mut messages: Vec<Message> = Vec::new();
        for thread_id in thread_ids {
            if messages.len() >= limit {
                break;
            }
            let thread = client.thread(&thread_id, 100).await?;
            let mut items: Vec<&Item> = thread.items.iter().collect();
            items.reverse();
            for item in items {
                let message = mappers::message(item, &thread_id);
                if term.is_empty() || message.text.to_lowercase().contains(&term) {
                    messages.push(message);
                    if messages.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.unwrap_or_default().to_owned(),
            messages,
            has_more: false,
            newest_first: false,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let response = self
            .client()?
            .send_text(&request.chat_id, &request.text)
            .await?;
        let message_id = response
            .payload
            .as_ref()
            .and_then(|payload| payload.items.first())
            .and_then(|item| item.item_id.clone())
            .filter(|id| !id.is_empty());
        let chat_id = response
            .payload
            .as_ref()
            .and_then(|payload| payload.thread_id.clone())
            .unwrap_or_else(|| request.chat_id.clone());
        Ok(SendResult {
            provider: PROVIDER,
            chat_id,
            message_id,
            sent_at: now_rfc3339(),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let client = self.client()?;
        let viewer = client.user_id().to_owned();
        let thread = client.thread(chat_id, 1).await?;
        Ok(thread
            .users
            .iter()
            .map(|user| mappers::member(user, &viewer))
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
impl AuthProvider for InstagramProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Extract]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let present = self.session().is_ok();
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
        let sessionid = options
            .params
            .get("sessionid")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Instagram login needs a sessionid".to_owned(),
            })?;
        let ds_user_id = options
            .params
            .get("ds_user_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Instagram login needs a ds_user_id".to_owned(),
            })?;
        let mut stored = serde_json::json!({
            "sessionid": sessionid,
            "ds_user_id": ds_user_id,
        });
        for key in ["device_id", "csrftoken"] {
            if let Some(value) = options
                .params
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                stored[key] = Value::String(value.to_owned());
            }
        }
        self.credentials.save(PROVIDER, DEFAULT_ACCOUNT, &stored)?;
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
