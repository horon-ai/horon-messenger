use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Mutex;

use hyper_messenger_core::{Error, ProviderKind, Result};

use crate::models::{
    DiscordGuild, DiscordGuildMember, DiscordMessage, DiscordSearchResponse, DiscordUser,
};

pub const DEFAULT_BASE_URL: &str = "https://discord.com/api/v10";
pub const BASE_URL_ENV: &str = "HYPER_MESSENGER_DISCORD_API_BASE";

const USER_AGENT: &str = "Discord/1.0.9210 Chrome/134.0.0.0 Electron/35.3.0";
const SUPER_PROPERTIES: &str = r#"{"os":"Mac OS X","browser":"Discord Client","release_channel":"stable","client_version":"1.0.9210","os_version":"24.3.0","os_arch":"arm64","app_arch":"arm64","system_locale":"en-US","browser_user_agent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9210 Chrome/134.0.0.0 Electron/35.3.0 Safari/537.36","browser_version":"134.0.0.0","client_build_number":366629,"native_build_number":64292,"client_event_source":null,"design_id":0}"#;
const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 100;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    remaining: i64,
    reset_at_ms: u64,
}

/// The Discord REST v10 client: token auth, super-properties, and rate-limit aware retries.
pub struct DiscordClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl DiscordClient {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|error| Error::Internal {
                message: format!("failed to build the Discord HTTP client: {error}"),
            })?;
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
            buckets: Mutex::new(HashMap::new()),
        })
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        let mut headers = HeaderMap::new();
        let super_properties =
            base64::engine::general_purpose::STANDARD.encode(SUPER_PROPERTIES.as_bytes());
        let entries = [
            ("authorization", self.token.clone()),
            ("user-agent", USER_AGENT.to_owned()),
            ("x-super-properties", super_properties),
            ("x-discord-locale", "en-US".to_owned()),
            ("x-discord-timezone", "UTC".to_owned()),
        ];
        for (name, value) in entries {
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(&value),
            ) {
                headers.insert(name, value);
            }
        }
        headers
    }

    fn bucket_key(method: &Method, path: &str) -> String {
        let route = path.split('?').next().unwrap_or(path);
        let mut normalized = route.to_owned();
        for (prefix, placeholder) in [
            ("/channels/", "/channels/{id}"),
            ("/guilds/", "/guilds/{id}"),
            ("/users/", "/users/{id}"),
        ] {
            if let Some(rest) = route.strip_prefix(prefix)
                && let Some((_, tail)) = rest.split_once('/')
            {
                normalized = format!("{placeholder}/{tail}");
            }
        }
        format!("{method}:{normalized}")
    }

    async fn wait_for_bucket(&self, key: &str) {
        let bucket = { self.buckets.lock().await.get(key).copied() };
        if let Some(bucket) = bucket
            && bucket.remaining <= 0
            && bucket.reset_at_ms > now_ms()
        {
            tokio::time::sleep(Duration::from_millis(bucket.reset_at_ms - now_ms())).await;
        }
    }

    async fn update_bucket(&self, key: &str, headers: &reqwest::header::HeaderMap) {
        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        if let (Some(remaining), Some(reset)) = (remaining, reset) {
            let reset_at_ms = (reset * 1000.0) as u64;
            self.buckets.lock().await.insert(
                key.to_owned(),
                Bucket {
                    remaining,
                    reset_at_ms,
                },
            );
        }
    }

    fn map_status(provider: ProviderKind, status: StatusCode, message: String) -> Error {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Error::AuthFailed { provider, message }
            }
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

    fn error_message(body: &Value, status: StatusCode) -> String {
        body.get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP {status}"))
    }

    async fn send_once(
        &self,
        method: &Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self
            .http
            .request(method.clone(), url)
            .headers(self.headers());
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().await.map_err(|error| Error::Network {
            provider: ProviderKind::Discord,
            message: error.to_string(),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let key = Self::bucket_key(&method, path);
        let mut attempt = 0u32;
        loop {
            self.wait_for_bucket(&key).await;
            let response = self.send_once(&method, path, body.as_ref()).await?;
            let status = response.status();
            self.update_bucket(&key, response.headers()).await;

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

            if status.is_server_error() && attempt < MAX_RETRIES {
                tokio::time::sleep(Duration::from_millis(BASE_BACKOFF_MS * 2u64.pow(attempt)))
                    .await;
                attempt += 1;
                continue;
            }

            let value: Value = response.json().await.unwrap_or(Value::Null);
            if !status.is_success() {
                return Err(Self::map_status(
                    ProviderKind::Discord,
                    status,
                    Self::error_message(&value, status),
                ));
            }
            return serde_json::from_value(value).map_err(|error| Error::Protocol {
                provider: ProviderKind::Discord,
                message: error.to_string(),
            });
        }
    }

    async fn request_ok(&self, method: Method, path: &str, body: Option<Value>) -> Result<()> {
        let _: Value = self.request(method, path, body).await?;
        Ok(())
    }

    pub async fn test_auth(&self) -> Result<DiscordUser> {
        self.request(Method::GET, "/users/@me", None).await
    }

    pub async fn list_guilds(&self) -> Result<Vec<DiscordGuild>> {
        self.request(Method::GET, "/users/@me/guilds", None).await
    }

    pub async fn get_guild(&self, guild_id: &str) -> Result<DiscordGuild> {
        self.request(Method::GET, &format!("/guilds/{guild_id}"), None)
            .await
    }

    pub async fn list_messages(
        &self,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<DiscordMessage>> {
        self.request(
            Method::GET,
            &format!("/channels/{channel_id}/messages?limit={limit}"),
            None,
        )
        .await
    }

    pub async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<DiscordMessage> {
        let mut body = serde_json::json!({ "content": content });
        if let Some(reply_to) = reply_to {
            body["message_reference"] = serde_json::json!({ "message_id": reply_to });
        }
        self.request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(body),
        )
        .await
    }

    pub async fn list_members(&self, guild_id: &str) -> Result<Vec<DiscordGuildMember>> {
        self.request(
            Method::GET,
            &format!("/guilds/{guild_id}/members?limit=1000"),
            None,
        )
        .await
    }

    pub async fn search_messages(
        &self,
        guild_id: &str,
        query: &str,
    ) -> Result<DiscordSearchResponse> {
        let encoded = urlencode(query);
        self.request(
            Method::GET,
            &format!("/guilds/{guild_id}/messages/search?content={encoded}"),
            None,
        )
        .await
    }

    pub async fn ack_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        self.request_ok(
            Method::POST,
            &format!("/channels/{channel_id}/messages/{message_id}/ack"),
            Some(serde_json::json!({ "token": Value::Null })),
        )
        .await
    }
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
    fn urlencode_escapes_spaces_and_symbols() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("safe-._~"), "safe-._~");
    }

    #[test]
    fn bucket_key_normalizes_ids() {
        assert_eq!(
            DiscordClient::bucket_key(&Method::GET, "/channels/123/messages?limit=50"),
            "GET:/channels/{id}/messages"
        );
        assert_eq!(
            DiscordClient::bucket_key(&Method::GET, "/users/@me"),
            "GET:/users/@me"
        );
    }

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:9999") };
        let client = DiscordClient::new("token").unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:9999");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }
}
