use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, QueryString, Request};

use crate::models::{ApiEnvelope, BotUser, ChatMember, Message, TelegramChat, Update};

/// The default Telegram Bot API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.telegram.org";
/// The environment variable that overrides the Bot API base URL.
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_TELEGRAMBOT_API_BASE";

const PROVIDER: ProviderKind = ProviderKind::TelegramBot;

/// The Telegram Bot API client: the token rides in the URL path with rate-limit aware retries.
pub struct TelegramBotClient {
    http: HttpClient,
    base_url: String,
    token: String,
}

impl TelegramBotClient {
    /// Builds a client for `token`, reading the base URL override from the environment.
    pub fn new(http: HttpClient, token: impl Into<String>) -> Self {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
        }
    }

    fn method_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.token, method)
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str("application/json") {
            headers.insert("content-type", value);
        }
        headers
    }

    async fn call(
        &self,
        method: &str,
        params: &[(&str, String)],
        body: Option<Value>,
        idempotent: bool,
    ) -> Result<Value> {
        let mut url = self.method_url(method);
        if !params.is_empty() {
            let mut query = QueryString::new();
            for (key, value) in params {
                query.push(key, value);
            }
            url = format!("{url}?{}", query.build());
        }
        let http_method = if body.is_some() { Method::POST } else { Method::GET };
        let mut request = Request::new(http_method, url, PROVIDER)
            .headers(Self::headers())
            .bucket(format!("telegrambot:{method}"))
            .idempotent(idempotent);
        if let Some(body) = body {
            request = request.body(body);
        }
        let value = self.http.request_value(request).await?;
        let envelope = envelope_from_value(value)?;
        if !envelope.ok {
            return Err(map_api_error(&envelope));
        }
        Ok(envelope.result.unwrap_or(Value::Null))
    }

    async fn call_typed<T: DeserializeOwned>(
        &self,
        method: &str,
        params: &[(&str, String)],
        body: Option<Value>,
        idempotent: bool,
    ) -> Result<T> {
        let value = self.call(method, params, body, idempotent).await?;
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider: PROVIDER,
            message: format!("unexpected `{method}` result: {error}"),
        })
    }

    /// Calls `getMe`, returning the bot account.
    pub async fn get_me(&self) -> Result<BotUser> {
        self.call_typed("getMe", &[], None, true).await
    }

    /// Calls `getUpdates`, returning the pending updates.
    pub async fn get_updates(&self) -> Result<Vec<Update>> {
        self.call_typed(
            "getUpdates",
            &[("limit", "100".to_owned()), ("timeout", "0".to_owned())],
            None,
            true,
        )
        .await
    }

    /// Calls `getChat`, returning the chat's full info.
    pub async fn get_chat(&self, chat_id: &str) -> Result<TelegramChat> {
        self.call_typed("getChat", &[("chat_id", chat_id.to_owned())], None, true)
            .await
    }

    /// Calls `getChatAdministrators`, returning the chat's administrators.
    pub async fn get_chat_administrators(&self, chat_id: &str) -> Result<Vec<ChatMember>> {
        self.call_typed("getChatAdministrators", &[("chat_id", chat_id.to_owned())], None, true)
            .await
    }

    /// Calls `sendMessage`; the call is non-idempotent and is never retried automatically.
    pub async fn send_message(&self, chat_id: &str, text: &str, reply_to: Option<&str>) -> Result<Message> {
        let mut body = serde_json::json!({ "chat_id": chat_id, "text": text });
        if let Some(reply_to) = reply_to.and_then(|value| value.parse::<i64>().ok()) {
            body["reply_to_message_id"] = serde_json::json!(reply_to);
        }
        self.call_typed("sendMessage", &[], Some(body), false).await
    }
}

fn envelope_from_value(value: Value) -> Result<ApiEnvelope> {
    serde_json::from_value(value).map_err(|error| Error::Protocol {
        provider: PROVIDER,
        message: format!("malformed Telegram Bot API response: {error}"),
    })
}

fn map_api_error(envelope: &ApiEnvelope) -> Error {
    let message = envelope
        .description
        .clone()
        .unwrap_or_else(|| "Telegram Bot API error".to_owned());
    match envelope.error_code {
        Some(401) | Some(403) => Error::AuthFailed {
            provider: PROVIDER,
            message,
        },
        Some(404) => Error::NotFound {
            provider: PROVIDER,
            resource: message,
        },
        Some(429) => Error::RateLimited {
            provider: PROVIDER,
            retry_after_ms: envelope
                .parameters
                .as_ref()
                .and_then(|parameters| parameters.retry_after)
                .map(|seconds| seconds.saturating_mul(1000)),
        },
        _ => Error::Protocol {
            provider: PROVIDER,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horon_messenger_core::ErrorCode;
    use serde_json::json;

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:8811") };
        let client = TelegramBotClient::new(HttpClient::shared(), "mock");
        assert_eq!(client.base_url, "http://127.0.0.1:8811");
        assert_eq!(client.method_url("getMe"), "http://127.0.0.1:8811/botmock/getMe");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }

    #[test]
    fn ok_true_envelope_yields_the_result() {
        let envelope = envelope_from_value(json!({ "ok": true, "result": { "id": 42 } })).unwrap();
        assert!(envelope.ok);
        assert_eq!(envelope.result.unwrap()["id"], json!(42));
    }

    #[test]
    fn malformed_bodies_map_to_protocol_errors() {
        for value in [Value::Null, json!({}), json!({ "result": 1 })] {
            let error = envelope_from_value(value).unwrap_err();
            assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
        }
    }

    #[test]
    fn ok_false_maps_to_typed_errors() {
        let cases = [
            (
                json!({ "ok": false, "error_code": 401, "description": "Unauthorized" }),
                ErrorCode::AuthFailed,
            ),
            (
                json!({ "ok": false, "error_code": 403, "description": "Forbidden" }),
                ErrorCode::AuthFailed,
            ),
            (
                json!({ "ok": false, "error_code": 404, "description": "Not Found" }),
                ErrorCode::NotFound,
            ),
            (
                json!({ "ok": false, "error_code": 429, "description": "Too Many" }),
                ErrorCode::RateLimited,
            ),
            (
                json!({ "ok": false, "error_code": 400, "description": "Bad Request" }),
                ErrorCode::Protocol,
            ),
            (json!({ "ok": false, "description": "no code" }), ErrorCode::Protocol),
        ];
        for (value, code) in cases {
            let envelope = envelope_from_value(value).unwrap();
            let error = map_api_error(&envelope);
            assert_eq!(error.code(), code, "for {error:?}");
        }
    }

    #[test]
    fn rate_limit_carries_retry_after() {
        let envelope = envelope_from_value(json!({
            "ok": false,
            "error_code": 429,
            "description": "Too Many Requests",
            "parameters": { "retry_after": 3 }
        }))
        .unwrap();
        let error = map_api_error(&envelope);
        assert!(
            matches!(
                error,
                Error::RateLimited {
                    retry_after_ms: Some(3000),
                    ..
                }
            ),
            "got {error:?}"
        );
    }
}
