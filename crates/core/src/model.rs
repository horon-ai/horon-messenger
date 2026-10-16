use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::kind::ProviderKind;

fn is_null(value: &Value) -> bool {
    value.is_null()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub provider: ProviderKind,
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    Direct,
    Group,
    Channel,
    OpenChat,
    Thread,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub provider: ProviderKind,
    pub id: String,
    pub name: String,
    pub kind: ChatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default)]
    pub is_self: bool,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub provider: ProviderKind,
    pub id: String,
    pub chat_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    #[serde(default)]
    pub text: String,
    pub sent_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePage {
    pub provider: ProviderKind,
    pub chat_id: String,
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub newest_first: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendRequest {
    pub chat_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Unverified,
    ServerAccepted,
    Confirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub provider: ProviderKind,
    pub chat_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub sent_at: String,
    pub accepted: bool,
    pub delivery: Delivery,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub provider: ProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    pub chats: Vec<Chat>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Message,
    Read,
    Typing,
    Unknown,
}

/// One thing that happened on a provider, emitted by a `listen` stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub provider: ProviderKind,
    pub chat_id: String,
    pub kind: EventKind,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(default, skip_serializing_if = "is_null")]
    pub extra: Value,
}

/// The stream a provider returns from `listen`.
pub type EventStream = std::pin::Pin<Box<dyn futures_core::Stream<Item = crate::error::Result<Event>> + Send>>;
