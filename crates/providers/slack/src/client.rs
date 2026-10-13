use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request, urlencode};

use crate::models::{
    SlackAuthTest, SlackChannel, SlackConversationsList, SlackHistory, SlackMembers,
    SlackPostMessage, SlackSearchMessages, SlackUser,
};

pub const DEFAULT_BASE_URL: &str = "https://slack.com/api";
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_SLACK_API_BASE";

const PROVIDER: ProviderKind = ProviderKind::Slack;
const PAGE_LIMIT: usize = 200;

/// Renders the `Cookie` header value, accepting a bare value or a full `d=` pair.
pub fn cookie_header(cookie: &str) -> String {
    if cookie.starts_with("d=") {
        cookie.to_owned()
    } else {
        format!("d={cookie}")
    }
}

/// Maps a Slack `error` code to the typed provider error.
pub fn map_slack_error(error: &str) -> Error {
    match error {
        "invalid_auth" | "not_authed" | "account_inactive" | "token_revoked" | "token_expired"
        | "invalid_token" => Error::AuthFailed {
            provider: PROVIDER,
            message: error.to_owned(),
        },
        "channel_not_found" | "message_not_found" | "user_not_found" | "file_not_found" => {
            Error::NotFound {
                provider: PROVIDER,
                resource: error.to_owned(),
            }
        }
        "ratelimited" => Error::RateLimited {
            provider: PROVIDER,
            retry_after_ms: None,
        },
        other => Error::Protocol {
            provider: PROVIDER,
            message: format!("slack error `{other}`"),
        },
    }
}

/// Rejects a Slack body whose `ok` flag is not `true`, even at HTTP 200.
pub fn check_ok(value: &Value) -> Result<()> {
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => {
            let error = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error");
            Err(map_slack_error(error))
        }
        None => Err(Error::Protocol {
            provider: PROVIDER,
            message: "slack response is missing the `ok` field".to_owned(),
        }),
    }
}

/// Deserializes a Slack body, mapping a shape mismatch to a protocol error.
pub fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::Protocol {
        provider: PROVIDER,
        message: error.to_string(),
    })
}

/// The Slack Web API client: user token plus `d=` cookie, JSON bodies.
pub struct SlackClient {
    http: HttpClient,
    base_url: String,
    token: String,
    cookie: Option<String>,
}

impl SlackClient {
    /// Builds a client, defaulting the base URL to `https://slack.com/api`.
    pub fn new(http: HttpClient, token: impl Into<String>, cookie: Option<String>) -> Result<Self> {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.into(),
            cookie,
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.token)) {
            headers.insert("authorization", value);
        }
        if let Some(cookie) = &self.cookie
            && let Ok(value) = HeaderValue::from_str(&cookie_header(cookie))
        {
            headers.insert("cookie", value);
        }
        if let Ok(value) = HeaderValue::from_str("application/json") {
            headers.insert("content-type", value);
        }
        headers
    }

    async fn call(
        &self,
        method: Method,
        endpoint: &str,
        params: &[(&str, String)],
        body: Option<Value>,
        idempotent: bool,
    ) -> Result<Value> {
        let mut url = format!("{}{}", self.base_url, endpoint);
        if !params.is_empty() {
            let query = params
                .iter()
                .map(|(key, value)| format!("{}={}", urlencode(key), urlencode(value)))
                .collect::<Vec<_>>()
                .join("&");
            url.push('?');
            url.push_str(&query);
        }
        let mut request = Request::new(method, url, PROVIDER)
            .headers(self.headers())
            .bucket(endpoint)
            .idempotent(idempotent);
        if let Some(body) = body {
            request = request.body(body);
        }
        let value = self.http.request_value(request).await?;
        check_ok(&value)?;
        Ok(value)
    }

    pub async fn auth_test(&self) -> Result<SlackAuthTest> {
        let value = self
            .call(Method::GET, "/auth.test", &[], None, true)
            .await?;
        decode(value)
    }

    pub async fn conversations_list(&self) -> Result<Vec<SlackChannel>> {
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params: Vec<(&str, String)> = vec![
                ("limit", PAGE_LIMIT.to_string()),
                ("types", "public_channel,private_channel,mpim,im".to_owned()),
            ];
            if let Some(cursor) = &cursor {
                params.push(("cursor", cursor.clone()));
            }
            let value = self
                .call(Method::GET, "/conversations.list", &params, None, true)
                .await?;
            let page: SlackConversationsList = decode(value)?;
            channels.extend(page.channels);
            match page.response_metadata.and_then(|meta| meta.next_cursor) {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }
        Ok(channels)
    }

    pub async fn conversations_info(&self, channel: &str) -> Result<SlackChannel> {
        let value = self
            .call(
                Method::GET,
                "/conversations.info",
                &[("channel", channel.to_owned())],
                None,
                true,
            )
            .await?;
        decode(value.get("channel").cloned().unwrap_or(Value::Null))
    }

    pub async fn conversations_history(
        &self,
        channel: &str,
        limit: usize,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<SlackHistory> {
        let mut params: Vec<(&str, String)> = vec![
            ("channel", channel.to_owned()),
            ("limit", limit.to_string()),
        ];
        if let Some(oldest) = oldest {
            params.push(("oldest", oldest.to_owned()));
        }
        if let Some(latest) = latest {
            params.push(("latest", latest.to_owned()));
        }
        let value = self
            .call(Method::GET, "/conversations.history", &params, None, true)
            .await?;
        decode(value)
    }

    pub async fn conversations_members(&self, channel: &str) -> Result<Vec<String>> {
        let mut members = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params: Vec<(&str, String)> = vec![
                ("channel", channel.to_owned()),
                ("limit", PAGE_LIMIT.to_string()),
            ];
            if let Some(cursor) = &cursor {
                params.push(("cursor", cursor.clone()));
            }
            let value = self
                .call(Method::GET, "/conversations.members", &params, None, true)
                .await?;
            let page: SlackMembers = decode(value)?;
            members.extend(page.members);
            match page.response_metadata.and_then(|meta| meta.next_cursor) {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }
        Ok(members)
    }

    pub async fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SlackPostMessage> {
        let mut body = serde_json::json!({ "channel": channel, "text": text });
        if let Some(thread_ts) = thread_ts {
            body["thread_ts"] = Value::String(thread_ts.to_owned());
        }
        let value = self
            .call(Method::POST, "/chat.postMessage", &[], Some(body), false)
            .await?;
        decode(value)
    }

    pub async fn users_info(&self, user: &str) -> Result<SlackUser> {
        let value = self
            .call(
                Method::GET,
                "/users.info",
                &[("user", user.to_owned())],
                None,
                true,
            )
            .await?;
        decode(value.get("user").cloned().unwrap_or(Value::Null))
    }

    pub async fn search_messages(&self, query: &str, count: usize) -> Result<SlackSearchMessages> {
        let params: Vec<(&str, String)> = vec![
            ("query", query.to_owned()),
            ("sort", "timestamp".to_owned()),
            ("sort_dir", "desc".to_owned()),
            ("count", count.to_string()),
        ];
        let value = self
            .call(Method::GET, "/search.messages", &params, None, true)
            .await?;
        decode(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horon_messenger_core::ErrorCode;

    #[test]
    fn headers_carry_bearer_token_and_cookie() {
        let client =
            SlackClient::new(HttpClient::shared(), "xoxc-abc", Some("secret-cookie".to_owned()))
                .unwrap();
        let headers = client.headers();
        assert_eq!(headers.get("authorization").unwrap(), "Bearer xoxc-abc");
        assert_eq!(headers.get("cookie").unwrap(), "d=secret-cookie");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn cookie_header_accepts_a_prefixed_pair() {
        assert_eq!(cookie_header("abc"), "d=abc");
        assert_eq!(cookie_header("d=abc"), "d=abc");
    }

    #[test]
    fn headers_omit_cookie_when_absent() {
        let client = SlackClient::new(HttpClient::shared(), "xoxc-abc", None).unwrap();
        assert!(client.headers().get("cookie").is_none());
    }

    #[test]
    fn ok_false_at_http_200_maps_to_typed_errors() {
        let auth =
            check_ok(&serde_json::json!({ "ok": false, "error": "invalid_auth" })).unwrap_err();
        assert_eq!(auth.code(), ErrorCode::AuthFailed);
        let not_found = check_ok(&serde_json::json!({ "ok": false, "error": "channel_not_found" }))
            .unwrap_err();
        assert_eq!(not_found.code(), ErrorCode::NotFound);
        let rate =
            check_ok(&serde_json::json!({ "ok": false, "error": "ratelimited" })).unwrap_err();
        assert_eq!(rate.code(), ErrorCode::RateLimited);
        let other = check_ok(&serde_json::json!({ "ok": false, "error": "boom" })).unwrap_err();
        assert_eq!(other.code(), ErrorCode::Protocol);
    }

    #[test]
    fn ok_true_passes_and_missing_ok_is_protocol() {
        assert!(check_ok(&serde_json::json!({ "ok": true })).is_ok());
        let missing = check_ok(&serde_json::json!({ "channels": [] })).unwrap_err();
        assert_eq!(missing.code(), ErrorCode::Protocol);
    }

    #[test]
    fn missing_required_field_decodes_to_protocol() {
        let error = decode::<SlackAuthTest>(serde_json::json!({ "ok": true })).unwrap_err();
        assert_eq!(error.code(), ErrorCode::Protocol);
    }

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:9997") };
        let client = SlackClient::new(HttpClient::shared(), "token", None).unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:9997");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }
}
