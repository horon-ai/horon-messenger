//! WhatsApp Bot provider: a Rust port of the agent-messenger WhatsApp Cloud API client.

mod client;
mod models;

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, ConfigPaths,
    CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome, Profile,
    Provider, ProviderKind, Result, SendRequest, SendResult, now_rfc3339,
};

use client::WhatsAppBotClient;

const PROVIDER: ProviderKind = ProviderKind::WhatsAppBot;
const DEFAULT_ACCOUNT: &str = "default";
const PHONE_NUMBER_ID_ENV: &str = "HORON_MESSENGER_WHATSAPPBOT_PHONE_NUMBER_ID";
const ACCESS_TOKEN_ENV: &str = "HORON_MESSENGER_WHATSAPPBOT_ACCESS_TOKEN";
const E2E_PHONE_NUMBER_ID_ENV: &str = "E2E_WHATSAPPBOT_PHONE_NUMBER_ID";
const E2E_ACCESS_TOKEN_ENV: &str = "E2E_WHATSAPPBOT_ACCESS_TOKEN";

#[derive(Debug, Clone)]
struct Credentials {
    phone_number_id: String,
    access_token: String,
}

/// The WhatsApp Bot provider: Cloud API bearer authentication.
#[derive(Debug, Clone)]
pub struct WhatsAppBotProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl WhatsAppBotProvider {
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

    fn resolve(&self) -> Result<Option<Credentials>> {
        if let Some(credentials) = credentials_from_env() {
            return Ok(Some(credentials));
        }
        let stored = self.credentials.load(PROVIDER, DEFAULT_ACCOUNT)?;
        Ok(stored.as_ref().and_then(credentials_from_value))
    }

    fn resolve_credentials(&self) -> Result<Credentials> {
        self.resolve()?
            .ok_or(Error::AuthRequired { provider: PROVIDER })
    }

    fn client(&self) -> Result<WhatsAppBotClient> {
        let credentials = self.resolve_credentials()?;
        WhatsAppBotClient::new(credentials.phone_number_id, credentials.access_token)
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn credentials_from_env() -> Option<Credentials> {
    for (phone_key, token_key) in [
        (PHONE_NUMBER_ID_ENV, ACCESS_TOKEN_ENV),
        (E2E_PHONE_NUMBER_ID_ENV, E2E_ACCESS_TOKEN_ENV),
    ] {
        let phone_number_id = non_empty(std::env::var(phone_key).ok());
        let access_token = non_empty(std::env::var(token_key).ok());
        if let (Some(phone_number_id), Some(access_token)) = (phone_number_id, access_token) {
            return Some(Credentials {
                phone_number_id,
                access_token,
            });
        }
    }
    None
}

fn credentials_from_value(value: &Value) -> Option<Credentials> {
    let phone_number_id = value
        .get("phone_number_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    Some(Credentials {
        phone_number_id: phone_number_id.to_owned(),
        access_token: access_token.to_owned(),
    })
}

#[async_trait]
impl Provider for WhatsAppBotProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI | Capabilities::SEND_MESSAGE
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        let credentials = self.resolve_credentials()?;
        let phone = WhatsAppBotClient::new(
            credentials.phone_number_id.clone(),
            credentials.access_token,
        )?
        .verify_phone_number()
        .await?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: credentials.phone_number_id,
            display_name: phone.verified_name,
            username: None,
            extra: Value::Null,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let response = self
            .client()?
            .send_text(&request.chat_id, &request.text)
            .await?;
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: response.messages.first().map(|message| message.id.clone()),
            sent_at: now_rfc3339(),
            accepted: true,
            delivery: Delivery::ServerAccepted,
            extra: Value::Null,
        })
    }
}

#[async_trait]
impl AuthProvider for WhatsAppBotProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Token]
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
        let phone_number_id = options
            .params
            .get("phone_number_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "WhatsApp Bot login needs a phone_number_id".to_owned(),
            })?;
        let access_token = options
            .params
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "WhatsApp Bot login needs an access_token".to_owned(),
            })?;
        self.credentials.save(
            PROVIDER,
            DEFAULT_ACCOUNT,
            &serde_json::json!({
                "phone_number_id": phone_number_id,
                "access_token": access_token,
            }),
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

    fn provider() -> WhatsAppBotProvider {
        WhatsAppBotProvider::new(ConfigPaths::at(
            std::env::temp_dir().join("hm-whatsappbot-caps"),
        ))
    }

    #[test]
    fn capabilities_are_exactly_whoami_and_send_message() {
        assert_eq!(
            provider().capabilities(),
            Capabilities::WHOAMI | Capabilities::SEND_MESSAGE
        );
    }

    #[test]
    fn stored_credentials_require_both_fields() {
        let both = serde_json::json!({ "phone_number_id": "phone", "access_token": "token" });
        assert!(credentials_from_value(&both).is_some());
        assert!(
            credentials_from_value(&serde_json::json!({ "phone_number_id": "phone" })).is_none()
        );
        assert!(credentials_from_value(&serde_json::json!({ "access_token": "token" })).is_none());
        let empty = serde_json::json!({ "phone_number_id": "", "access_token": "token" });
        assert!(credentials_from_value(&empty).is_none());
    }

    #[test]
    fn env_credentials_take_precedence_over_stored_credentials() {
        let dir = std::env::temp_dir().join(format!("hm-whatsappbot-auth-{}", std::process::id()));
        let provider = WhatsAppBotProvider::new(ConfigPaths::at(&dir));
        provider
            .credentials
            .save(
                PROVIDER,
                DEFAULT_ACCOUNT,
                &serde_json::json!({ "phone_number_id": "stored-phone", "access_token": "stored-token" }),
            )
            .unwrap();

        for key in [
            PHONE_NUMBER_ID_ENV,
            ACCESS_TOKEN_ENV,
            E2E_PHONE_NUMBER_ID_ENV,
            E2E_ACCESS_TOKEN_ENV,
        ] {
            unsafe { std::env::remove_var(key) };
        }
        assert_eq!(
            provider.resolve().unwrap().unwrap().phone_number_id,
            "stored-phone"
        );

        unsafe { std::env::set_var(PHONE_NUMBER_ID_ENV, "env-phone") };
        unsafe { std::env::set_var(ACCESS_TOKEN_ENV, "env-token") };
        let resolved = provider.resolve().unwrap().unwrap();
        assert_eq!(resolved.phone_number_id, "env-phone");
        assert_eq!(resolved.access_token, "env-token");

        unsafe { std::env::set_var(E2E_PHONE_NUMBER_ID_ENV, "e2e-phone") };
        unsafe { std::env::set_var(E2E_ACCESS_TOKEN_ENV, "e2e-token") };
        unsafe { std::env::remove_var(PHONE_NUMBER_ID_ENV) };
        unsafe { std::env::remove_var(ACCESS_TOKEN_ENV) };
        assert_eq!(
            provider.resolve().unwrap().unwrap().phone_number_id,
            "e2e-phone"
        );

        for key in [
            PHONE_NUMBER_ID_ENV,
            ACCESS_TOKEN_ENV,
            E2E_PHONE_NUMBER_ID_ENV,
            E2E_ACCESS_TOKEN_ENV,
        ] {
            unsafe { std::env::remove_var(key) };
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
