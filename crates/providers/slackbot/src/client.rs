use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request};

use crate::models::{AuthTest, Channel, ChannelInfo, ChannelList, History, MemberList, PostedMessage, UserInfo};

pub const DEFAULT_BASE_URL: &str = "https://slack.com/api";
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_SLACKBOT_API_BASE";

const BUCKET_KEY: &str = "slackbot:global";

/// Maps a Slack `error` code to a typed provider error; an auth code is `AuthFailed`, everything else `Protocol`.
pub fn map_slack_error(code: &str) -> Error {
    match code {
        "invalid_auth" | "not_authed" | "account_inactive" => Error::AuthFailed {
            provider: ProviderKind::SlackBot,
            message: format!("slack: {code}"),
        },
        _ => Error::Protocol {
            provider: ProviderKind::SlackBot,
            message: format!("slack: {code}"),
        },
    }
}

/// Turns a Slack `{ok, error}` body into a result: a `false` or absent `ok` is an error even at HTTP 200.
pub fn check_response(value: Value) -> Result<Value> {
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(value),
        _ => {
            let code = value.get("error").and_then(Value::as_str).unwrap_or("api_error");
            Err(map_slack_error(code))
        }
    }
}

/// The Slack Web API client: bot-token auth and rate-limit aware retries.
pub struct SlackBotClient {
    http: HttpClient,
    base_url: String,
    token: String,
}

impl SlackBotClient {
    pub fn new(http: HttpClient, token: impl Into<String>) -> Self {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.token)) {
            headers.insert("authorization", value);
        }
        if let Ok(value) = HeaderValue::from_str("application/json; charset=utf-8") {
            headers.insert("content-type", value);
        }
        headers
    }

    async fn call<T: DeserializeOwned>(&self, endpoint: &str, body: Value, idempotent: bool) -> Result<T> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let request = Request::new(Method::POST, url, ProviderKind::SlackBot)
            .headers(self.headers())
            .body(body)
            .bucket(BUCKET_KEY)
            .idempotent(idempotent);
        let value = check_response(self.http.request_value(request).await?)?;
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider: ProviderKind::SlackBot,
            message: error.to_string(),
        })
    }

    pub async fn test_auth(&self) -> Result<AuthTest> {
        self.call("auth.test", serde_json::json!({}), true).await
    }

    pub async fn list_channels(&self, limit: usize) -> Result<Vec<Channel>> {
        let body = serde_json::json!({
            "types": "public_channel,private_channel",
            "limit": limit,
        });
        let list: ChannelList = self.call("conversations.list", body, true).await?;
        Ok(list.channels)
    }

    pub async fn get_channel(&self, channel: &str) -> Result<Channel> {
        let body = serde_json::json!({ "channel": channel });
        let info: ChannelInfo = self.call("conversations.info", body, true).await?;
        Ok(info.channel)
    }

    pub async fn history(&self, channel: &str, limit: usize) -> Result<History> {
        let body = serde_json::json!({ "channel": channel, "limit": limit });
        self.call("conversations.history", body, true).await
    }

    pub async fn post_message(&self, channel: &str, text: &str, thread_ts: Option<&str>) -> Result<PostedMessage> {
        let mut body = serde_json::json!({ "channel": channel, "text": text });
        if let Some(thread_ts) = thread_ts {
            body["thread_ts"] = Value::String(thread_ts.to_owned());
        }
        self.call("chat.postMessage", body, false).await
    }

    pub async fn members(&self, channel: &str) -> Result<Vec<String>> {
        let body = serde_json::json!({ "channel": channel });
        let list: MemberList = self.call("conversations.members", body, true).await?;
        Ok(list.members)
    }

    pub async fn user_info(&self, user: &str) -> Result<UserInfo> {
        let body = serde_json::json!({ "user": user });
        self.call("users.info", body, true).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horon_messenger_core::ErrorCode;

    #[test]
    fn ok_false_maps_auth_codes_to_auth_failed() {
        for code in ["invalid_auth", "not_authed", "account_inactive"] {
            let value = serde_json::json!({ "ok": false, "error": code });
            let error = check_response(value).unwrap_err();
            assert_eq!(error.code(), ErrorCode::AuthFailed, "for {code}");
        }
    }

    #[test]
    fn ok_false_maps_other_codes_to_protocol() {
        let value = serde_json::json!({ "ok": false, "error": "channel_not_found" });
        assert_eq!(check_response(value).unwrap_err().code(), ErrorCode::Protocol);
    }

    #[test]
    fn missing_ok_is_protocol() {
        let value = serde_json::json!({ "channels": [] });
        assert_eq!(check_response(value).unwrap_err().code(), ErrorCode::Protocol);
    }

    #[test]
    fn ok_true_passes_the_body_through() {
        let value = serde_json::json!({ "ok": true, "ts": "1.2" });
        assert_eq!(check_response(value).unwrap()["ts"], "1.2");
    }

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:8813") };
        let client = SlackBotClient::new(HttpClient::shared(), "xoxb-test");
        assert_eq!(client.base_url, "http://127.0.0.1:8813");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }
}
