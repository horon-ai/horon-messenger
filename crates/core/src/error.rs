use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::kind::ProviderKind;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A machine-readable class for an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unsupported,
    NotImplemented,
    NotRegistered,
    AuthRequired,
    AuthFailed,
    NotFound,
    Ambiguous,
    RateLimited,
    Network,
    Protocol,
    InvalidInput,
    Io,
    Config,
    Internal,
}

/// The error every provider operation returns; platform failures carry the
/// [`ProviderKind`] that produced them.
#[derive(Debug, Error)]
pub enum Error {
    #[error("provider {provider} does not support `{operation}`")]
    Unsupported {
        provider: ProviderKind,
        operation: &'static str,
    },

    #[error("provider {provider} has not implemented `{operation}` yet")]
    NotImplemented {
        provider: ProviderKind,
        operation: &'static str,
    },

    #[error("provider {provider} is not registered in this build")]
    NotRegistered { provider: ProviderKind },

    #[error("authentication is required for provider {provider}")]
    AuthRequired { provider: ProviderKind },

    #[error("authentication failed for provider {provider}: {message}")]
    AuthFailed { provider: ProviderKind, message: String },

    #[error("{resource} was not found for provider {provider}")]
    NotFound { provider: ProviderKind, resource: String },

    #[error("{resource} is ambiguous for provider {provider}: {message}")]
    Ambiguous {
        provider: ProviderKind,
        resource: String,
        message: String,
    },

    #[error("provider {provider} is rate limited")]
    RateLimited {
        provider: ProviderKind,
        retry_after_ms: Option<u64>,
    },

    #[error("network error talking to provider {provider}: {message}")]
    Network { provider: ProviderKind, message: String },

    #[error("protocol error for provider {provider}: {message}")]
    Protocol { provider: ProviderKind, message: String },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("configuration error: {message}")]
    Config { message: String },

    #[error("internal error: {message}")]
    Internal { message: String },
}

impl Error {
    pub fn code(&self) -> ErrorCode {
        match self {
            Error::Unsupported { .. } => ErrorCode::Unsupported,
            Error::NotImplemented { .. } => ErrorCode::NotImplemented,
            Error::NotRegistered { .. } => ErrorCode::NotRegistered,
            Error::AuthRequired { .. } => ErrorCode::AuthRequired,
            Error::AuthFailed { .. } => ErrorCode::AuthFailed,
            Error::NotFound { .. } => ErrorCode::NotFound,
            Error::Ambiguous { .. } => ErrorCode::Ambiguous,
            Error::RateLimited { .. } => ErrorCode::RateLimited,
            Error::Network { .. } => ErrorCode::Network,
            Error::Protocol { .. } => ErrorCode::Protocol,
            Error::InvalidInput { .. } => ErrorCode::InvalidInput,
            Error::Io(_) => ErrorCode::Io,
            Error::Json(_) => ErrorCode::Internal,
            Error::Config { .. } => ErrorCode::Config,
            Error::Internal { .. } => ErrorCode::Internal,
        }
    }

    /// Whether retrying the same call could plausibly succeed.
    pub fn retryable(&self) -> bool {
        matches!(self, Error::RateLimited { .. } | Error::Network { .. } | Error::Io(_))
    }

    pub fn provider(&self) -> Option<ProviderKind> {
        match self {
            Error::Unsupported { provider, .. }
            | Error::NotImplemented { provider, .. }
            | Error::NotRegistered { provider }
            | Error::AuthRequired { provider }
            | Error::AuthFailed { provider, .. }
            | Error::NotFound { provider, .. }
            | Error::Ambiguous { provider, .. }
            | Error::RateLimited { provider, .. }
            | Error::Network { provider, .. }
            | Error::Protocol { provider, .. } => Some(*provider),
            Error::InvalidInput { .. }
            | Error::Io(_)
            | Error::Json(_)
            | Error::Config { .. }
            | Error::Internal { .. } => None,
        }
    }
}
