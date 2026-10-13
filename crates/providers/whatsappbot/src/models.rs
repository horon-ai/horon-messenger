use serde::Deserialize;

/// The Graph API response to a sent message.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageResponse {
    #[serde(default)]
    pub messages: Vec<SentMessage>,
}

/// A sent message identifier returned by the Graph API.
#[derive(Debug, Clone, Deserialize)]
pub struct SentMessage {
    pub id: String,
}

/// The Graph API phone-number metadata used to verify a credential.
#[derive(Debug, Clone, Deserialize)]
pub struct PhoneNumber {
    #[serde(default)]
    pub verified_name: Option<String>,
}
