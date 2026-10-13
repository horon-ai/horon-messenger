//! WhatsApp provider, driven from a linked WhatsApp account over a Node sidecar.

mod models;
mod sidecar;

use std::path::PathBuf;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use hyper_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatQuery,
    ConfigPaths, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome, Member,
    MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result, SendRequest, SendResult,
    Snapshot, now_rfc3339,
};

use models::{
    SidecarAuthStatus, SidecarChat, SidecarLogin, SidecarMessage, chat_from_sidecar,
    member_from_sidecar, message_from_sidecar,
};
use sidecar::Sidecar;

const PROVIDER: ProviderKind = ProviderKind::WhatsApp;
const DEFAULT_ACCOUNT: &str = "default";

/// The WhatsApp provider, driven from a linked WhatsApp account over a Node sidecar.
#[derive(Debug, Clone)]
pub struct WhatsAppProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    sidecar: Sidecar,
}

impl WhatsAppProvider {
    /// Builds the provider over the given config paths.
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            sidecar: Sidecar::resolve(),
        }
    }

    /// The config paths the provider reads credentials and state from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The credential store backing this provider.
    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn auth_dir(&self) -> PathBuf {
        self.paths.accounts_dir(PROVIDER).join(DEFAULT_ACCOUNT)
    }

    async fn auth_status(&self) -> Result<SidecarAuthStatus> {
        self.request("auth.status", json!({ "auth_dir": self.auth_dir() }))
            .await
    }

    async fn require_auth(&self) -> Result<SidecarAuthStatus> {
        let status = self.auth_status().await?;
        if status.authenticated {
            Ok(status)
        } else {
            Err(Error::AuthRequired { provider: PROVIDER })
        }
    }

    async fn request<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let value = self.sidecar.request(method, params).await?;
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider: PROVIDER,
            message: format!("{method} returned an unexpected shape: {error}"),
        })
    }

    async fn chats(&self, params: Value) -> Result<Vec<SidecarChat>> {
        self.request("chats.list", params).await
    }
}

#[async_trait]
impl Provider for WhatsAppProvider {
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
        let status = self.require_auth().await?;
        let account = status
            .account
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: account.id,
            display_name: account.name,
            username: account.phone_number,
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        self.require_auth().await?;
        let chats = self
            .chats(json!({
                "auth_dir": self.auth_dir(),
                "search": &query.search,
                "limit": &query.limit,
            }))
            .await?;
        Ok(chats.iter().map(chat_from_sidecar).collect())
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        self.require_auth().await?;
        let chats = self
            .chats(json!({ "auth_dir": self.auth_dir(), "chat_id": chat_id }))
            .await?;
        chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .map(chat_from_sidecar)
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_id}"),
            })
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        self.require_auth().await?;
        let limit = query.limit.unwrap_or(50);
        let messages: Vec<SidecarMessage> = self
            .request(
                "messages.list",
                json!({
                    "auth_dir": self.auth_dir(),
                    "chat_id": chat_id,
                    "limit": limit,
                    "search": &query.search,
                    "before": &query.before,
                    "after": &query.after,
                }),
            )
            .await?;
        let has_more = messages.len() >= limit;
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages: messages.iter().map(message_from_sidecar).collect(),
            has_more,
            newest_first: false,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        self.require_auth().await?;
        let message: SidecarMessage = self
            .request(
                "message.send",
                json!({
                    "auth_dir": self.auth_dir(),
                    "chat_id": &request.chat_id,
                    "text": &request.text,
                    "reply_to": &request.reply_to,
                }),
            )
            .await?;
        let sent_at = message
            .timestamp
            .clone()
            .filter(|timestamp| !timestamp.is_empty())
            .unwrap_or_else(now_rfc3339);
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
        self.require_auth().await?;
        let chats = self
            .chats(json!({ "auth_dir": self.auth_dir(), "chat_id": chat_id }))
            .await?;
        let chat = chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_id}"),
            })?;
        Ok(chat.members.iter().map(member_from_sidecar).collect())
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
impl AuthProvider for WhatsAppProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Qr, AuthMethod::Login]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let status = self.auth_status().await?;
        let account = status.account;
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated: status.authenticated,
            current: status.authenticated.then_some(DEFAULT_ACCOUNT.to_owned()),
            accounts: account
                .map(|account| {
                    vec![AccountSummary {
                        account_id: DEFAULT_ACCOUNT.to_owned(),
                        display_name: account.name,
                        method: Some(AuthMethod::Qr),
                        current: true,
                        extra: Value::Null,
                    }]
                })
                .unwrap_or_default(),
        })
    }

    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome> {
        let method = options.method.or_else(|| {
            options
                .params
                .get("method")
                .and_then(Value::as_str)
                .and_then(|value| match value {
                    "qr" => Some(AuthMethod::Qr),
                    "login" | "pairing" => Some(AuthMethod::Login),
                    _ => None,
                })
        });
        let phone_number = options
            .params
            .get("phone_number")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let login: SidecarLogin = self
            .request(
                "auth.login",
                json!({
                    "auth_dir": self.auth_dir(),
                    "method": method.map(|method| match method {
                        AuthMethod::Login => "login",
                        _ => "qr",
                    }),
                    "phone_number": &phone_number,
                }),
            )
            .await?;
        Ok(LoginOutcome {
            provider: PROVIDER,
            authenticated: login.authenticated,
            account_id: login
                .account_id
                .or_else(|| login.authenticated.then_some(DEFAULT_ACCOUNT.to_owned())),
            next_action: login.next_action,
            message: login.message,
        })
    }

    async fn logout(&self, _account_id: Option<&str>) -> Result<()> {
        match std::fs::remove_dir_all(self.auth_dir()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> WhatsAppProvider {
        WhatsAppProvider::new(ConfigPaths::at(
            std::env::temp_dir().join("hm-whatsapp-caps"),
        ))
    }

    #[test]
    fn capabilities_cover_the_advertised_operations() {
        assert_eq!(
            provider().capabilities(),
            Capabilities::WHOAMI
                | Capabilities::LIST_CHATS
                | Capabilities::GET_CHAT
                | Capabilities::LIST_MESSAGES
                | Capabilities::SEND_MESSAGE
                | Capabilities::LIST_MEMBERS
                | Capabilities::SNAPSHOT
        );
    }

    #[test]
    fn auth_dir_is_the_default_account_directory() {
        let paths = ConfigPaths::at("/tmp/hm-whatsapp-auth");
        let provider = WhatsAppProvider::new(paths.clone());
        assert_eq!(
            provider.auth_dir(),
            paths.accounts_dir(ProviderKind::WhatsApp).join("default")
        );
    }

    #[test]
    fn provider_exposes_the_auth_contract() {
        assert!(provider().auth().is_some());
    }
}
