use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::kind::ProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Extract,
    Login,
    Qr,
    Token,
    BotToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<AuthMethod>,
    #[serde(default)]
    pub current: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub provider: ProviderKind,
    pub authenticated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<AccountSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<AuthMethod>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginOutcome {
    pub provider: ProviderKind,
    pub authenticated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Credential acquisition, separate from the message operations so a provider can
/// support reading and sending while delegating sign-in to a CLI-only flow.
#[async_trait]
pub trait AuthProvider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind;

    fn methods(&self) -> &'static [AuthMethod];

    async fn status(&self) -> Result<AuthStatus>;

    async fn login(&self, _options: &LoginOptions) -> Result<LoginOutcome> {
        Err(Error::NotImplemented {
            provider: self.kind(),
            operation: "login",
        })
    }

    async fn logout(&self, _account_id: Option<&str>) -> Result<()> {
        Err(Error::NotImplemented {
            provider: self.kind(),
            operation: "logout",
        })
    }
}
