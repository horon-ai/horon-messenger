//! The Instagram mobile API client: fingerprint headers, session cookies and a request timeout.

use std::time::Duration;

use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request, urlencode};

use crate::models::{
    BroadcastResponse, CurrentUserResponse, InboxResponse, Thread, ThreadResponse,
};
use crate::session::InstagramSession;

/// The default Instagram mobile API base URL.
pub const DEFAULT_BASE_URL: &str = "https://i.instagram.com/api/v1";
/// The environment variable overriding the Instagram API base URL.
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_INSTAGRAM_API_BASE";
/// The Instagram Android app id sent as `X-IG-App-ID`.
pub const APP_ID: &str = "567067343352427";
/// The timeout applied to every Instagram request.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

const PROVIDER: ProviderKind = ProviderKind::Instagram;
const IG_VERSION: &str = "428.0.0.47.67";
const DEVICE_STRING: &str =
    "34/14; 480dpi; 1344x2992; Google/google; Pixel 8 Pro; husky; husky; en_US; 719358530";

/// Builds the Instagram Android user agent the API expects.
pub fn user_agent() -> String {
    format!("Instagram {IG_VERSION} Android ({DEVICE_STRING})")
}

fn base_url_from(override_value: Option<String>) -> String {
    override_value
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::Protocol {
        provider: PROVIDER,
        message: error.to_string(),
    })
}

fn status_error(value: &Value, status: &str) -> Error {
    let raw = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or(status);
    let lower = raw.to_lowercase();
    if lower.contains("login_required") || lower.contains("not logged in") {
        Error::AuthFailed {
            provider: PROVIDER,
            message: raw.to_owned(),
        }
    } else {
        Error::Protocol {
            provider: PROVIDER,
            message: format!("instagram error `{status}`: {raw}"),
        }
    }
}

/// Rejects an Instagram body whose `status` field is not `ok`.
pub fn check_status(value: &Value) -> Result<()> {
    match value.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(()),
        Some(status) => Err(status_error(value, status)),
        None => Err(Error::Protocol {
            provider: PROVIDER,
            message: "instagram response is missing the `status` field".to_owned(),
        }),
    }
}

/// The Instagram mobile API client, authenticated with session cookies.
pub struct InstagramClient {
    http: HttpClient,
    base_url: String,
    session: InstagramSession,
    timeout: Duration,
}

impl InstagramClient {
    /// Builds a client, defaulting the base URL to `https://i.instagram.com/api/v1`.
    pub fn new(session: InstagramSession) -> Result<Self> {
        Ok(Self {
            http: HttpClient::new()?,
            base_url: base_url_from(std::env::var(BASE_URL_ENV).ok()),
            session,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// The numeric user id of the authenticated account.
    pub fn user_id(&self) -> &str {
        &self.session.ds_user_id
    }

    /// Assembles the headers every Instagram request carries.
    pub fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        insert(&mut headers, "user-agent", &user_agent());
        insert(&mut headers, "x-ig-app-id", APP_ID);
        insert(&mut headers, "x-ig-www-claim", "0");
        insert(
            &mut headers,
            "ig-intended-user-id",
            &self.session.ds_user_id,
        );
        insert(
            &mut headers,
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        );
        insert(&mut headers, "accept", "*/*");
        if let Some(device_id) = &self.session.device_id {
            insert(&mut headers, "x-ig-device-id", device_id);
        }
        insert(&mut headers, "cookie", &self.session.cookie_header());
        headers
    }

    async fn request(&self, method: Method, endpoint: &str, body: Option<Value>) -> Result<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        let request = if method == Method::GET {
            Request::get(url, PROVIDER)
        } else {
            Request::post(url, PROVIDER)
        }
        .headers(self.headers())
        .bucket(endpoint.to_owned());
        let request = match body {
            Some(body) => request.body(body),
            None => request,
        };
        match tokio::time::timeout(self.timeout, self.http.request_value(request)).await {
            Ok(result) => result,
            Err(_) => Err(Error::Network {
                provider: PROVIDER,
                message: format!(
                    "instagram request to {endpoint} timed out after {}s; the IP may be flagged or the device fingerprint rejected",
                    self.timeout.as_secs()
                ),
            }),
        }
    }

    async fn call(&self, endpoint: &str) -> Result<Value> {
        let value = self.request(Method::GET, endpoint, None).await?;
        check_status(&value)?;
        Ok(value)
    }

    async fn call_body(&self, endpoint: &str, body: Value) -> Result<Value> {
        let value = self.request(Method::POST, endpoint, Some(body)).await?;
        check_status(&value)?;
        Ok(value)
    }

    /// Fetches the authenticated account.
    pub async fn current_user(&self) -> Result<CurrentUserResponse> {
        decode(self.call("/accounts/current_user/?edit=true").await?)
    }

    /// Fetches the direct-message inbox threads.
    pub async fn inbox(&self, limit: usize) -> Result<Vec<Thread>> {
        let value = self
            .call(&format!("/direct_v2/inbox/?limit={limit}"))
            .await?;
        let response: InboxResponse = decode(value)?;
        Ok(response
            .inbox
            .map(|inbox| inbox.threads)
            .unwrap_or_default())
    }

    /// Fetches one thread with its recent items.
    pub async fn thread(&self, thread_id: &str, limit: usize) -> Result<Thread> {
        let value = self
            .call(&format!("/direct_v2/threads/{thread_id}/?limit={limit}"))
            .await?;
        let response: ThreadResponse = decode(value)?;
        response.thread.ok_or_else(|| Error::NotFound {
            provider: PROVIDER,
            resource: format!("thread {thread_id}"),
        })
    }

    /// Broadcasts a text message to a thread.
    pub async fn send_text(&self, thread_id: &str, text: &str) -> Result<BroadcastResponse> {
        let form = format!(
            "thread_ids={}&text={}&action=send_item&client_context={}",
            urlencode(&format!("[{thread_id}]")),
            urlencode(text),
            uuid::Uuid::new_v4(),
        );
        let value = self
            .call_body("/direct_v2/threads/broadcast/text/", Value::String(form))
            .await?;
        decode(value)
    }
}

#[cfg(test)]
impl InstagramClient {
    pub(crate) fn test_client(
        session: InstagramSession,
        base_url: String,
        timeout: Duration,
    ) -> Self {
        Self {
            http: HttpClient::new().expect("http client"),
            base_url,
            session,
            timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> InstagramSession {
        InstagramSession {
            sessionid: "SID".to_owned(),
            ds_user_id: "4242".to_owned(),
            device_id: Some("android-mock-device".to_owned()),
            csrftoken: Some("TOKEN".to_owned()),
        }
    }

    #[test]
    fn headers_carry_user_agent_app_id_cookie_and_intended_user() {
        let client = InstagramClient::test_client(
            session(),
            "http://127.0.0.1:1".to_owned(),
            DEFAULT_TIMEOUT,
        );
        let headers = client.headers();
        assert!(
            headers["user-agent"]
                .to_str()
                .unwrap()
                .starts_with("Instagram ")
        );
        assert_eq!(headers["x-ig-app-id"], APP_ID);
        assert_eq!(headers["x-ig-device-id"], "android-mock-device");
        assert_eq!(headers["ig-intended-user-id"], "4242");
        assert_eq!(
            headers["cookie"],
            "sessionid=SID; ds_user_id=4242; csrftoken=TOKEN"
        );
    }

    #[test]
    fn headers_omit_the_device_id_when_absent() {
        let mut session = session();
        session.device_id = None;
        let client =
            InstagramClient::test_client(session, "http://127.0.0.1:1".to_owned(), DEFAULT_TIMEOUT);
        assert!(client.headers().get("x-ig-device-id").is_none());
    }

    #[test]
    fn base_url_from_honours_the_override_and_trims_slashes() {
        assert_eq!(base_url_from(None), DEFAULT_BASE_URL);
        assert_eq!(
            base_url_from(Some("http://127.0.0.1:8831/api/v1/".to_owned())),
            "http://127.0.0.1:8831/api/v1"
        );
    }

    #[test]
    fn check_status_accepts_ok_and_rejects_missing_or_fail() {
        assert!(check_status(&serde_json::json!({ "status": "ok" })).is_ok());
        let missing = check_status(&serde_json::json!({ "inbox": {} })).unwrap_err();
        assert_eq!(missing.code(), horon_messenger_core::ErrorCode::Protocol);
        let fail =
            check_status(&serde_json::json!({ "status": "fail", "message": "login_required" }))
                .unwrap_err();
        assert_eq!(fail.code(), horon_messenger_core::ErrorCode::AuthFailed);
        let other =
            check_status(&serde_json::json!({ "status": "fail", "message": "boom" })).unwrap_err();
        assert_eq!(other.code(), horon_messenger_core::ErrorCode::Protocol);
    }

    #[tokio::test]
    async fn a_stalled_request_maps_to_a_network_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });
        let client = InstagramClient::test_client(
            session(),
            format!("http://{addr}"),
            Duration::from_millis(200),
        );
        let error = client.current_user().await.unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(error.to_string().contains("timed out"), "{error}");
    }

    #[tokio::test]
    async fn a_non_json_success_body_maps_to_a_protocol_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
                let body = "notjs";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
            }
        });
        let client =
            InstagramClient::test_client(session(), format!("http://{addr}"), DEFAULT_TIMEOUT);
        let error = client.current_user().await.unwrap_err();
        assert_eq!(
            error.code(),
            horon_messenger_core::ErrorCode::Protocol,
            "got {error:?}"
        );
    }
}
