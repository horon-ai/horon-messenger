use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request, urlencode};

use crate::models::{Channel, ChannelAccount, ChannelManager, ChannelMessage, SearchHit};

pub const DEFAULT_BASE_URL: &str = "https://desk-api.channel.io";
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_CHANNELTALK_API_BASE";

const BUCKET_KEY: &str = "channeltalk:global";

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
                message: format!("chat id must be <channel_id>/<chat_type>/<chat_id>, got `{value}`"),
            });
        }
        let chat_type = ChatType::parse(parts[1]).ok_or_else(|| Error::InvalidInput {
            message: format!("chat type must be group, user-chat or direct-chat, got `{}`", parts[1]),
        })?;
        Ok(Self {
            channel_id: parts[0].to_owned(),
            chat_type,
            chat_id: parts[2].to_owned(),
        })
    }
}

/// The Channel Talk desk-api client: cookie auth and rate-limit aware retries.
pub struct ChannelTalkClient {
    http: HttpClient,
    base_url: String,
    account_cookie: String,
    session_cookie: Option<String>,
}

impl ChannelTalkClient {
    pub fn new(http: HttpClient, account_cookie: impl Into<String>, session_cookie: Option<String>) -> Self {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            account_cookie: account_cookie.into(),
            session_cookie,
        }
    }

    fn cookie_header(&self) -> String {
        match &self.session_cookie {
            Some(session) => format!("x-account={}; ch-session-1={}", self.account_cookie, session),
            None => format!("x-account={}", self.account_cookie),
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&self.cookie_header()) {
            headers.insert("cookie", value);
        }
        if let Ok(value) = HeaderValue::from_str("application/json") {
            headers.insert("content-type", value);
        }
        headers
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        unwrap_key: Option<&str>,
        idempotent: bool,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::new(method, url, ProviderKind::ChannelTalk)
            .headers(self.headers())
            .bucket(BUCKET_KEY)
            .idempotent(idempotent);
        if let Some(body) = body {
            request = request.body(body);
        }
        let mut value = self.http.request_value(request).await?;
        if let Some(key) = unwrap_key
            && let Some(inner) = value.get(key)
        {
            value = inner.clone();
        }
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider: ProviderKind::ChannelTalk,
            message: error.to_string(),
        })
    }

    pub async fn get_account(&self) -> Result<ChannelAccount> {
        self.request(Method::GET, "/desk/account", None, Some("account"), true)
            .await
    }

    pub async fn list_channels(&self) -> Result<Vec<Channel>> {
        self.request(Method::GET, "/desk/channels", None, Some("channels"), true)
            .await
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<Channel> {
        self.request(
            Method::GET,
            &format!("/desk/channels/{channel_id}"),
            None,
            Some("channel"),
            true,
        )
        .await
    }

    pub async fn list_managers(&self, channel_id: &str) -> Result<Vec<ChannelManager>> {
        self.request(
            Method::GET,
            &format!("/desk/channels/{channel_id}/managers"),
            None,
            Some("managers"),
            true,
        )
        .await
    }

    pub async fn list_messages(&self, reference: &ChatRef, limit: usize) -> Result<Vec<ChannelMessage>> {
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
            true,
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
            false,
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
            true,
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
        let client = ChannelTalkClient::new(HttpClient::shared(), "cookie", None);
        assert_eq!(client.base_url, "http://127.0.0.1:9998");
        assert_eq!(client.cookie_header(), "x-account=cookie");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }
}
