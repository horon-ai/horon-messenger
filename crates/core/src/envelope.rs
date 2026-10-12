use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, ErrorCode, Result};
use crate::kind::ProviderKind;
use crate::now_rfc3339;

#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderKind>,
    pub generated_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Meta {
    pub fn now(provider: Option<ProviderKind>) -> Self {
        Self {
            provider,
            generated_at: now_rfc3339(),
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderKind>,
}

impl From<&Error> for ErrorBody {
    fn from(error: &Error) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
            retryable: error.retryable(),
            provider: error.provider(),
        }
    }
}

/// The wire shape every surface returns: `{ok: true, data, meta}` on success,
/// `{ok: false, error, meta}` on failure.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
    pub meta: Meta,
}

impl Envelope {
    pub fn success<T: Serialize>(data: T, meta: Meta) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self {
                ok: true,
                data: Some(value),
                error: None,
                meta,
            },
            Err(error) => Self::failure(
                &Error::Internal {
                    message: format!("failed to serialize response: {error}"),
                },
                meta,
            ),
        }
    }

    pub fn failure(error: &Error, meta: Meta) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(ErrorBody::from(error)),
            meta,
        }
    }

    pub fn from_result<T: Serialize>(result: Result<T>, meta: Meta) -> Self {
        match result {
            Ok(value) => Self::success(value, meta),
            Err(error) => Self::failure(&error, meta),
        }
    }

    pub fn into_value(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| FALLBACK.to_owned())
    }

    pub fn to_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| FALLBACK.to_owned())
    }
}

const FALLBACK: &str = r#"{"ok":false,"error":{"code":"INTERNAL","message":"failed to render envelope","retryable":false}}"#;
