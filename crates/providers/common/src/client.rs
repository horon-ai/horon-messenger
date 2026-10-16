use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use horon_messenger_core::{Error, ProviderKind, Result};
use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::status::{error_message, map_http_status};

/// The number of automatic retries the client performs for a retryable request.
pub const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 100;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(BASE_BACKOFF_MS * 2u64.pow(attempt))
}

fn retry_after(headers: &HeaderMap) -> f64 {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0)
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    remaining: i64,
    reset_at_ms: u64,
}

/// A prepared request: method, URL, headers, optional JSON body and retry policy.
#[derive(Debug, Clone)]
pub struct Request {
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Option<Value>,
    provider: ProviderKind,
    bucket_key: String,
    idempotent: bool,
}

impl Request {
    /// Starts a request for `method` and `url`, idempotent by default.
    pub fn new(method: Method, url: impl Into<String>, provider: ProviderKind) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HeaderMap::new(),
            body: None,
            provider,
            bucket_key: String::new(),
            idempotent: true,
        }
    }

    /// Starts an idempotent GET request.
    pub fn get(url: impl Into<String>, provider: ProviderKind) -> Self {
        Self::new(Method::GET, url, provider)
    }

    /// Starts a non-idempotent POST request.
    pub fn post(url: impl Into<String>, provider: ProviderKind) -> Self {
        Self::new(Method::POST, url, provider).idempotent(false)
    }

    /// Sets the request headers.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    /// Sets the JSON request body.
    pub fn body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets the rate-limit bucket key.
    pub fn bucket(mut self, key: impl Into<String>) -> Self {
        self.bucket_key = key.into();
        self
    }

    /// Marks whether a transmitted request may be retried automatically.
    pub fn idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent = idempotent;
        self
    }
}

/// The process-global client, built once on first use.
static SHARED: OnceLock<HttpClient> = OnceLock::new();

/// A retry- and rate-limit-aware HTTP client shared by the messenger providers.
///
/// Cloning is cheap and shares the underlying connection pool and the
/// rate-limit bucket map, so callers reuse one set of connections.
#[derive(Debug, Clone)]
pub struct HttpClient {
    http: reqwest::Client,
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
    max_retries: u32,
}

impl HttpClient {
    /// Builds a client with the default retry budget.
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder().build().map_err(|error| Error::Internal {
            message: format!("failed to build the HTTP client: {error}"),
        })?;
        Ok(Self::with_client(http))
    }

    /// Wraps an existing [`reqwest::Client`] with the default retry budget.
    pub fn with_client(http: reqwest::Client) -> Self {
        Self {
            http,
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_retries: MAX_RETRIES,
        }
    }

    /// Returns the process-global client, building it once on first use.
    ///
    /// Every returned clone shares one [`reqwest::Client`] connection pool and
    /// one rate-limit bucket map. Building is infallible: if the fallible
    /// builder ever fails, the plain [`reqwest::Client::new`] is used instead.
    pub fn shared() -> Self {
        SHARED
            .get_or_init(|| {
                let http = reqwest::Client::builder()
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                Self::with_client(http)
            })
            .clone()
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

    async fn update_bucket(&self, key: &str, headers: &HeaderMap) {
        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        if remaining.is_none() && reset.is_none() {
            return;
        }
        let mut buckets = self.buckets.lock().await;
        let entry = buckets.entry(key.to_owned()).or_insert(Bucket {
            remaining: 0,
            reset_at_ms: 0,
        });
        if let Some(remaining) = remaining {
            entry.remaining = remaining;
        }
        if let Some(reset) = reset {
            entry.reset_at_ms = if reset > 1_000_000_000_000.0 {
                reset as u64
            } else {
                (reset * 1000.0) as u64
            };
        }
    }

    fn transport_error(request: &Request, attempt: u32, error: &reqwest::Error) -> Error {
        let message = if request.idempotent {
            format!("request to {} failed after {attempt} retries: {error}", request.url)
        } else {
            format!(
                "request to {} was sent but the outcome is unknown and it is not retried automatically (non-idempotent call): {error}",
                request.url
            )
        };
        Error::Network {
            provider: request.provider,
            message,
        }
    }

    async fn execute(&self, request: &Request) -> Result<(StatusCode, Vec<u8>)> {
        let mut attempt = 0u32;
        loop {
            self.wait_for_bucket(&request.bucket_key).await;
            let mut builder = self
                .http
                .request(request.method.clone(), &request.url)
                .headers(request.headers.clone());
            if let Some(body) = request.body.as_ref() {
                builder = builder.json(body);
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(error) => {
                    if request.idempotent && attempt < self.max_retries {
                        tokio::time::sleep(backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(Self::transport_error(request, attempt, &error));
                }
            };
            let status = response.status();
            self.update_bucket(&request.bucket_key, response.headers()).await;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries {
                let delay = retry_after(response.headers());
                tokio::time::sleep(Duration::from_millis((delay * 1000.0) as u64)).await;
                attempt += 1;
                continue;
            }

            if status.is_server_error() && request.idempotent && attempt < self.max_retries {
                tokio::time::sleep(backoff(attempt)).await;
                attempt += 1;
                continue;
            }

            match response.bytes().await {
                Ok(bytes) => return Ok((status, bytes.to_vec())),
                Err(error) => {
                    if request.idempotent && attempt < self.max_retries {
                        tokio::time::sleep(backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(Self::transport_error(request, attempt, &error));
                }
            }
        }
    }

    /// Sends a request and returns the parsed JSON body, applying the retry and rate-limit policy.
    pub async fn request_value(&self, request: Request) -> Result<Value> {
        let (status, bytes) = self.execute(&request).await?;
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(map_http_status(request.provider, status, error_message(&value, status)));
        }
        Ok(value)
    }

    /// Sends a request and deserializes the JSON body into `T`.
    pub async fn request<T: DeserializeOwned>(&self, request: Request) -> Result<T> {
        let provider = request.provider;
        let value = self.request_value(request).await?;
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider,
            message: error.to_string(),
        })
    }

    /// Sends an idempotent GET and deserializes the JSON body into `T`.
    pub async fn get<T: DeserializeOwned>(
        &self,
        url: impl Into<String>,
        headers: HeaderMap,
        provider: ProviderKind,
        bucket_key: impl Into<String>,
    ) -> Result<T> {
        self.request(Request::get(url, provider).headers(headers).bucket(bucket_key))
            .await
    }

    /// Sends a non-idempotent POST that is never retried after transmission.
    ///
    /// A transport failure returns `Error::Network` whose message states the
    /// outcome is unknown, because the request may already have been applied.
    pub async fn send<T: DeserializeOwned>(
        &self,
        url: impl Into<String>,
        headers: HeaderMap,
        body: Value,
        provider: ProviderKind,
        bucket_key: impl Into<String>,
    ) -> Result<T> {
        self.request(
            Request::post(url, provider)
                .headers(headers)
                .body(body)
                .bucket(bucket_key),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    enum Reply {
        Drop,
        Truncate,
        Respond {
            status: u16,
            reason: &'static str,
            headers: Vec<(&'static str, String)>,
            body: String,
        },
    }

    struct Server {
        base_url: String,
        hits: Arc<AtomicUsize>,
    }

    async fn spawn_server(replies: Vec<Reply>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_task = Arc::clone(&hits);
        tokio::spawn(async move {
            let mut index = 0usize;
            while let Ok((mut socket, _)) = listener.accept().await {
                hits_task.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 8192];
                let _ = socket.read(&mut buf).await;
                match replies.get(index) {
                    Some(Reply::Drop) => {
                        drop(socket);
                    }
                    Some(Reply::Truncate) => {
                        let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 500\r\nConnection: close\r\n\r\n{\"id\":\"partial";
                        let _ = socket.write_all(head.as_bytes()).await;
                        let _ = socket.shutdown().await;
                    }
                    Some(Reply::Respond {
                        status,
                        reason,
                        headers,
                        body,
                    }) => {
                        let mut response = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for (name, value) in headers {
                            response.push_str(&format!("{name}: {value}\r\n"));
                        }
                        response.push_str("\r\n");
                        response.push_str(body);
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.shutdown().await;
                    }
                    None => {
                        let body = "{}";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.shutdown().await;
                    }
                }
                index += 1;
            }
        });
        Server { base_url, hits }
    }

    fn respond(status: u16, reason: &'static str, body: &str) -> Reply {
        Reply::Respond {
            status,
            reason,
            headers: Vec::new(),
            body: body.to_owned(),
        }
    }

    /// Accepts connections forever, counting them, and answers every request
    /// on a kept-alive connection with a `200` JSON body.
    async fn spawn_keepalive_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(serve_keepalive(socket));
            }
        });
        (base_url, connections)
    }

    async fn serve_keepalive(socket: tokio::net::TcpStream) {
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);
        loop {
            let mut request_line = String::new();
            match reader.read_line(&mut request_line).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            if request_line.trim().is_empty() {
                continue;
            }
            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                match reader.read_line(&mut header).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                let trimmed = header.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                let lower = trimmed.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            if content_length > 0 {
                let mut body = vec![0u8; content_length];
                if reader.read_exact(&mut body).await.is_err() {
                    return;
                }
            }
            let body = "{}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            if write_half.write_all(response.as_bytes()).await.is_err() {
                return;
            }
            let _ = write_half.flush().await;
        }
    }

    #[tokio::test]
    async fn shared_client_reuses_one_connection() {
        let (base_url, connections) = spawn_keepalive_server().await;
        for _ in 0..5 {
            let client = HttpClient::shared();
            let value: Value = client
                .get(
                    format!("{base_url}/ping"),
                    HeaderMap::new(),
                    ProviderKind::Mock,
                    "shared",
                )
                .await
                .unwrap();
            assert_eq!(value, serde_json::json!({}));
        }
        assert_eq!(
            connections.load(Ordering::SeqCst),
            1,
            "five shared() GETs must reuse one keep-alive connection"
        );
    }

    #[tokio::test]
    async fn non_idempotent_send_is_not_retried_on_transport_failure() {
        let server = spawn_server(vec![Reply::Drop]).await;
        let client = HttpClient::new().unwrap();
        let error = client
            .send::<Value>(
                format!("{}/send", server.base_url),
                HeaderMap::new(),
                serde_json::json!({ "hello": "world" }),
                ProviderKind::Discord,
                "bucket",
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(
            error.to_string().contains("unknown"),
            "message must state the outcome is unknown: {error}"
        );
        assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn non_idempotent_send_is_not_retried_on_truncated_body() {
        let server = spawn_server(vec![Reply::Truncate]).await;
        let client = HttpClient::new().unwrap();
        let error = client
            .send::<Value>(
                format!("{}/send", server.base_url),
                HeaderMap::new(),
                serde_json::json!({ "hello": "world" }),
                ProviderKind::Discord,
                "bucket",
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(error.to_string().contains("unknown"), "{error}");
        assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn idempotent_get_is_retried_on_transport_failure() {
        let server = spawn_server(vec![Reply::Drop, respond(200, "OK", "{}")]).await;
        let client = HttpClient::new().unwrap();
        let value: Value = client
            .get(
                format!("{}/thing", server.base_url),
                HeaderMap::new(),
                ProviderKind::Discord,
                "bucket",
            )
            .await
            .unwrap();
        assert_eq!(value, serde_json::json!({}));
        assert_eq!(server.hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn too_many_requests_retries_then_succeeds() {
        let server = spawn_server(vec![
            Reply::Respond {
                status: 429,
                reason: "Too Many Requests",
                headers: vec![("Retry-After", "0".to_owned())],
                body: "{}".to_owned(),
            },
            respond(200, "OK", "{\"ok\":true}"),
        ])
        .await;
        let client = HttpClient::new().unwrap();
        let value: Value = client
            .send::<Value>(
                format!("{}/send", server.base_url),
                HeaderMap::new(),
                serde_json::json!({}),
                ProviderKind::ChannelTalk,
                "bucket",
            )
            .await
            .unwrap();
        assert_eq!(value, serde_json::json!({ "ok": true }));
        assert_eq!(server.hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn map_http_status_covers_every_class() {
        use horon_messenger_core::ErrorCode;
        let provider = ProviderKind::Discord;
        let cases = [
            (StatusCode::UNAUTHORIZED, ErrorCode::AuthFailed),
            (StatusCode::FORBIDDEN, ErrorCode::AuthFailed),
            (StatusCode::NOT_FOUND, ErrorCode::NotFound),
            (StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited),
            (StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::Network),
            (StatusCode::BAD_REQUEST, ErrorCode::Protocol),
        ];
        for (status, code) in cases {
            let error = map_http_status(provider, status, "boom".to_owned());
            assert_eq!(error.code(), code, "for status {status}");
        }
    }

    #[test]
    fn error_message_prefers_the_top_level_message() {
        let body = serde_json::json!({ "message": "top" });
        assert_eq!(error_message(&body, StatusCode::BAD_REQUEST), "top");
        let body = serde_json::json!({ "errors": [{ "message": "nested" }] });
        assert_eq!(error_message(&body, StatusCode::BAD_REQUEST), "nested");
        assert_eq!(
            error_message(&serde_json::json!({}), StatusCode::BAD_REQUEST),
            "HTTP 400 Bad Request"
        );
    }
}
