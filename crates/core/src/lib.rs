//! Core contracts shared by every horon-messenger provider and surface: the
//! [`Provider`] operation contract, the domain [`model`], config and credential
//! paths, and the [`Envelope`] wire shape the CLI, MCP server and API server speak.

pub mod auth;
pub mod capability;
pub mod config;
pub mod credential;
pub mod envelope;
pub mod error;
pub mod kind;
pub mod model;
pub mod provider;
pub mod registry;

pub use auth::{AccountSummary, AuthMethod, AuthProvider, AuthStatus, LoginOptions, LoginOutcome};
pub use capability::Capabilities;
pub use config::{APP_DIR_NAME, CONFIG_DIR_ENV, ConfigPaths};
pub use credential::{CredentialStore, FileCredentialStore};
pub use envelope::{Envelope, ErrorBody, Meta};
pub use error::{Error, ErrorCode, Result};
pub use kind::ProviderKind;
pub use model::{
    Attachment, Chat, ChatKind, ChatQuery, Delivery, Member, Message, MessagePage, MessageQuery,
    Profile, SendRequest, SendResult, Snapshot,
};
pub use provider::{DynProvider, Provider};
pub use registry::ProviderRegistry;

/// The current time as an RFC 3339 / ISO 8601 string in UTC, so every surface
/// stamps timestamps the same way.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
