use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Mutex;

use hyper_messenger_core::{Error, ProviderKind, Result};

use crate::models::{Channel, ChannelAccount, ChannelManager, ChannelMessage, SearchHit};

pub const DEFAULT_BASE_URL: &str = "https://desk-api.channel.io";
pub const BASE_URL_ENV: &str = "HYPER_MESSENGER_CHANNELTALK_API_BASE";

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatType {
    Group,
    UserChat,
    DirectChat,
}

impl ChatType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "group" => Some(ChatType::Group),
            "user-chat" => Some(ChatType::UserChat),
            "direct-chat" => Some(ChatType::DirectChat),
            _ => None,
        }
    }

    pub fn segment(self) -> &'static str {
        match self {
            ChatType::Group => "groups",
            ChatType::UserChat => "user-chats",
            ChatType::DirectChat => "direct-chats",
        }
    }
}

/// A chat reference parsed from the `<channel_id>/<chat_type>/<chat_id>` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRef {
    pub channel_id: String,
    pub chat_type: ChatType,
    pub chat_id: String,
}

impl ChatRef {
    pub fn parse(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split('/').collect();
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
            return Err(Error::InvalidInput {
                message: format!(
                    "chat id must be <channel_id>/<chat_type>/<chat_id>, got `{value}`"
                ),
            });
        }
        let chat_type = ChatType::parse(parts[1]).ok_or_else(|| Error::InvalidInput {
            message: format!(
                "chat type must be group, user-chat or direct-chat, got `{}`",
                parts[1]
            ),
        })?;
        Ok(Self {
            channel_id: parts[0].to_owned(),
            chat_type,
            chat_id: parts[2].to_owned(),
        })
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The Channel Talk desk-api client: cookie auth and rate-limit aware retries.
pub struct ChannelTalkClient {
    http: reqwest::Client,
    base_url: String,
    account_cookie: String,
    session_cookie: Option<String>,
    rate_limit: Mutex<(Option<i64>, u64)>,
}

impl ChannelTalkClient {
    pub fn new(account_cookie: impl Into<String>, session_cookie: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|error| Error::Internal {
                message: format!("failed to build the Channel Talk HTTP client: {error}"),
            })?;
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            account_cookie: account_cookie.into(),
            session_cookie,
            rate_limit: Mutex::new((None, 0)),
        })
    }

    fn cookie_header(&self) -> String {
        match &self.session_cookie {
            Some(session) => format!(
                "x-account={}; ch-session-1={}",
                self.account_cookie, session
            ),
            None => format!("x-account={}", self.account_cookie),
        }
    }

    async fn wait_for_rate_limit(&self) {
        let (remaining, reset_at_ms) = { *self.rate_limit.lock().await };
        if remaining == Some(0) && reset_at_ms > now_ms() {
            tokio::time::sleep(Duration::from_millis(reset_at_ms - now_ms())).await;
        }
    }

    async fn update_rate_limit(&self, headers: &reqwest::header::HeaderMap) {
        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        let mut guard = self.rate_limit.lock().await;
        if let Some(remaining) = remaining {
            guard.0 = Some(remaining);
        }
        if let Some(reset) = reset {
            let reset_at_ms = if reset > 1_000_000_000_000.0 {
                reset as u64
            } else {
                (reset * 1000.0) as u64
            };
            guard.1 = reset_at_ms;
        }
    }

    fn map_status(status: StatusCode, message: String) -> Error {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::AuthFailed {
                provider: ProviderKind::ChannelTalk,
                message,
            },
            StatusCode::NOT_FOUND => Error::NotFound {
                provider: ProviderKind::ChannelTalk,
                resource: message,
            },
            StatusCode::TOO_MANY_REQUESTS => Error::RateLimited {
                provider: ProviderKind::ChannelTalk,
                retry_after_ms: None,
            },
            status if status.is_server_error() => Error::Network {
                provider: ProviderKind::ChannelTalk,
                message,
            },
            _ => Error::Protocol {
                provider: ProviderKind::ChannelTalk,
                message,
            },
        }
    }

    fn error_message(body: &Value, status: StatusCode) -> String {
        body.get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
            .and_then(|first| first.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP {status}"))
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        unwrap_key: Option<&str>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt = 0u32;
        loop {
            self.wait_for_rate_limit().await;
            let mut request = self
                .http
                .request(method.clone(), &url)
                .header("Cookie", self.cookie_header())
                .header("Content-Type", "application/json");
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(|error| Error::Network {
                provider: ProviderKind::ChannelTalk,
                message: error.to_string(),
            })?;
            let status = response.status();
            self.update_rate_limit(response.headers()).await;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(1.0);
                tokio::time::sleep(Duration::from_millis((retry_after * 1000.0) as u64)).await;
                attempt += 1;
                continue;
            }

            if status.is_server_error() && method == Method::GET && attempt < MAX_RETRIES {
                tokio::time::sleep(Duration::from_millis(BASE_BACKOFF_MS * 2u64.pow(attempt)))
                    .await;
                attempt += 1;
                continue;
            }

            let mut value: Value = response.json().await.unwrap_or(Value::Null);
            if !status.is_success() {
                return Err(Self::map_status(
                    status,
                    Self::error_message(&value, status),
                ));
            }
            if let Some(key) = unwrap_key
                && let Some(inner) = value.get(key)
            {
                value = inner.clone();
            }
            return serde_json::from_value(value).map_err(|error| Error::Protocol {
                provider: ProviderKind::ChannelTalk,
                message: error.to_string(),
            });
        }
    }

    pub async fn get_account(&self) -> Result<ChannelAccount> {
        self.request(Method::GET, "/desk/account", None, Some("account"))
            .await
    }

    pub async fn list_channels(&self) -> Result<Vec<Channel>> {
        self.request(Method::GET, "/desk/channels", None, Some("channels"))
            .await
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<Channel> {
        self.request(
            Method::GET,
            &format!("/desk/channels/{channel_id}"),
            None,
            Some("channel"),
        )
        .await
    }

    pub async fn list_managers(&self, channel_id: &str) -> Result<Vec<ChannelManager>> {
        self.request(
            Method::GET,
            &format!("/desk/channels/{channel_id}/managers"),
            None,
            Some("managers"),
        )
        .await
    }

    pub async fn list_messages(
        &self,
        reference: &ChatRef,
        limit: usize,
    ) -> Result<Vec<ChannelMessage>> {
        self.request(
            Method::GET,
            &format!(
                "/desk/channels/{}/{}/{}/messages?limit={limit}&sortOrder=desc",
                reference.channel_id,
                reference.chat_type.segment(),
                reference.chat_id
            ),
            None,
            Some("messages"),
        )
        .await
    }

    pub async fn send_message(&self, reference: &ChatRef, text: &str) -> Result<ChannelMessage> {
        let body = serde_json::json!({
            "blocks": [{ "type": "text", "value": text }],
            "requestId": request_id(),
        });
        self.request(
            Method::POST,
            &format!(
                "/desk/channels/{}/{}/{}/messages",
                reference.channel_id,
                reference.chat_type.segment(),
                reference.chat_id
            ),
            Some(body),
            Some("message"),
        )
        .await
    }

    pub async fn search_messages(&self, channel_id: &str, query: &str) -> Result<Vec<SearchHit>> {
        let encoded = urlencode(query);
        self.request(
            Method::GET,
            &format!("/desk/channels/{channel_id}/team-chat/message/search?query={encoded}"),
            None,
            Some("hits"),
        )
        .await
    }
}

fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", nanos, std::process::id())
}

fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_ref_parses_the_composite_form() {
        let reference = ChatRef::parse("ws1/group/g1").unwrap();
        assert_eq!(reference.channel_id, "ws1");
        assert_eq!(reference.chat_type, ChatType::Group);
        assert_eq!(reference.chat_id, "g1");
        assert_eq!(ChatType::UserChat.segment(), "user-chats");
    }

    #[test]
    fn chat_ref_rejects_malformed_input() {
        assert!(ChatRef::parse("ws1/g1").is_err());
        assert!(ChatRef::parse("ws1/group/g1/extra").is_err());
        assert!(ChatRef::parse("ws1/bad/g1").is_err());
        assert!(ChatRef::parse("//").is_err());
    }

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:9998") };
        let client = ChannelTalkClient::new("cookie", None).unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:9998");
        assert_eq!(client.cookie_header(), "x-account=cookie");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }
}
