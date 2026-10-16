use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{HttpClient, Request};

use crate::models::{MessageResponse, PhoneNumber};

/// The production Graph API base URL.
pub const DEFAULT_BASE_URL: &str = "https://graph.facebook.com/v23.0";
/// The environment variable that overrides the Graph API base URL.
pub const BASE_URL_ENV: &str = "HORON_MESSENGER_WHATSAPPBOT_API_BASE";

const BUCKET_KEY: &str = "whatsappbot:global";

/// The WhatsApp Cloud API client: bearer authentication over the Graph API.
pub struct WhatsAppBotClient {
    http: HttpClient,
    base_url: String,
    phone_number_id: String,
    access_token: String,
}

impl WhatsAppBotClient {
    /// Builds a client for one phone number id, honouring the base-URL override.
    pub fn new(http: HttpClient, phone_number_id: impl Into<String>, access_token: impl Into<String>) -> Result<Self> {
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            phone_number_id: phone_number_id.into(),
            access_token: access_token.into(),
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.access_token)) {
            headers.insert("authorization", value);
        }
        if let Ok(value) = HeaderValue::from_str("application/json") {
            headers.insert("content-type", value);
        }
        headers
    }

    async fn request<T: DeserializeOwned>(&self, request: Request) -> Result<T> {
        let value = self.http.request_value(request).await?;
        serde_json::from_value(value).map_err(|error| Error::Protocol {
            provider: ProviderKind::WhatsAppBot,
            message: error.to_string(),
        })
    }

    /// Verifies the credential against the phone number's metadata.
    pub async fn verify_phone_number(&self) -> Result<PhoneNumber> {
        let url = format!("{}/{}?fields=verified_name", self.base_url, self.phone_number_id);
        self.request(
            Request::get(url, ProviderKind::WhatsAppBot)
                .headers(self.headers())
                .bucket(BUCKET_KEY),
        )
        .await
    }

    /// Sends a text message; the call is non-idempotent and never auto-retried.
    pub async fn send_text(&self, to: &str, text: &str) -> Result<MessageResponse> {
        let url = format!("{}/{}/messages", self.base_url, self.phone_number_id);
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": { "body": text, "preview_url": false },
        });
        self.request(
            Request::post(url, ProviderKind::WhatsAppBot)
                .headers(self.headers())
                .body(body)
                .bucket(BUCKET_KEY),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horon_messenger_core::ErrorCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn base_url_honours_the_env_override() {
        unsafe { std::env::set_var(BASE_URL_ENV, "http://127.0.0.1:9997") };
        let client = WhatsAppBotClient::new(HttpClient::shared(), "phone", "token").unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:9997");
        unsafe { std::env::remove_var(BASE_URL_ENV) };
    }

    #[tokio::test]
    async fn a_graph_error_body_maps_unauthorized_to_auth_failed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = r#"{"error":{"message":"Invalid OAuth access token","type":"OAuthException","code":190}}"#;
                let response = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        let mut client = WhatsAppBotClient::new(HttpClient::shared(), "phone-123", "bad-token").unwrap();
        client.base_url = format!("http://{addr}");
        let error = client.verify_phone_number().await.unwrap_err();
        assert_eq!(error.code(), ErrorCode::AuthFailed);
    }
}
