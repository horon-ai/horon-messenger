use horon_messenger_core::{Error, ProviderKind};
use reqwest::StatusCode;
use serde_json::Value;

/// Maps an HTTP status to the typed provider [`Error`] every surface returns.
pub fn map_http_status(provider: ProviderKind, status: StatusCode, message: String) -> Error {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::AuthFailed { provider, message },
        StatusCode::NOT_FOUND => Error::NotFound {
            provider,
            resource: message,
        },
        StatusCode::TOO_MANY_REQUESTS => Error::RateLimited {
            provider,
            retry_after_ms: None,
        },
        status if status.is_server_error() => Error::Network { provider, message },
        _ => Error::Protocol { provider, message },
    }
}

/// Extracts a human-readable message from a provider error body.
pub fn error_message(body: &Value, status: StatusCode) -> String {
    body.get("message")
        .and_then(Value::as_str)
        .or_else(|| {
            body.get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first())
                .and_then(|first| first.get("message"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned)
        .unwrap_or_else(|| format!("HTTP {status}"))
}
