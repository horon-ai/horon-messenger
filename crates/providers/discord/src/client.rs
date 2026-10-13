use base64::Engine;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request, urlencode};

use crate::models::{
    DiscordGuild, DiscordGuildMember, DiscordMessage, DiscordSearchResponse, DiscordUser,
};

pub const DEFAULT_BASE_URL: &str = "https://discord.com/api/v10";
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_DISCORD_API_BASE";

const USER_AGENT: &str = "Discord/1.0.9210 Chrome/134.0.0.0 Electron/35.3.0";
const SUPER_PROPERTIES: &str = r#"{"os":"Mac OS X","browser":"Discord Client","release_channel":"stable","client_version":"1.0.9210","os_version":"24.3.0","os_arch":"arm64","app_arch":"arm64","system_locale":"en-US","browser_user_agent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9210 Chrome/134.0.0.0 Electron/35.3.0 Safari/537.36","browser_version":"134.0.0.0","client_build_number":366629,"native_build_number":64292,"client_event_source":null,"design_id":0}"#;

/// The Discord REST v10 client: token auth, super-properties, and rate-limit aware retries.
pub struct DiscordClient {
    http: HttpClient,
    base_url: String,
    token: String,
}

impl DiscordClient {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http: HttpClient::new()?,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
        })
    }

    fn headers(&self) -> HeaderMap {
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

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotent: bool,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let key = Self::bucket_key(&method, path);
        let mut request = Request::new(method, url, ProviderKind::Discord)
            .headers(self.headers())
            .bucket(key)
            .idempotent(idempotent);
        if let Some(body) = body {
            request = request.body(body);
        }
        self.http.request(request).await
    }

    async fn request_ok(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotent: bool,
    ) -> Result<()> {
        let _: Value = self.request(method, path, body, idempotent).await?;
        Ok(())
    }

    pub async fn test_auth(&self) -> Result<DiscordUser> {
        self.request(Method::GET, "/users/@me", None, true).await
    }

    pub async fn list_guilds(&self) -> Result<Vec<DiscordGuild>> {
        self.request(Method::GET, "/users/@me/guilds", None, true)
            .await
    }

    pub async fn get_guild(&self, guild_id: &str) -> Result<DiscordGuild> {
        self.request(Method::GET, &format!("/guilds/{guild_id}"), None, true)
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
            true,
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
            false,
        )
        .await
    }

    pub async fn list_members(&self, guild_id: &str) -> Result<Vec<DiscordGuildMember>> {
        self.request(
            Method::GET,
            &format!("/guilds/{guild_id}/members?limit=1000"),
            None,
            true,
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
            true,
        )
        .await
    }

    pub async fn ack_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        self.request_ok(
            Method::POST,
            &format!("/channels/{channel_id}/messages/{message_id}/ack"),
            Some(serde_json::json!({ "token": Value::Null })),
            true,
        )
        .await
    }
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
