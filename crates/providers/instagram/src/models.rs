//! Wire shapes for the Instagram mobile API responses.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct CurrentUserResponse {
    #[serde(default)]
    pub user: Option<IgUser>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IgUser {
    #[serde(default)]
    pub pk: Option<Value>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub profile_pic_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InboxResponse {
    #[serde(default)]
    pub inbox: Option<Inbox>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Inbox {
    #[serde(default)]
    pub threads: Vec<Thread>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadResponse {
    #[serde(default)]
    pub thread: Option<Thread>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thread {
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub thread_title: Option<String>,
    #[serde(default)]
    pub thread_type: Option<String>,
    #[serde(default)]
    pub read_state: Option<u64>,
    #[serde(default)]
    pub users: Vec<IgUser>,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<Value>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub media: Option<Value>,
    #[serde(default)]
    pub media_share: Option<Value>,
    #[serde(default)]
    pub reel_share: Option<Value>,
    #[serde(default)]
    pub link: Option<Value>,
    #[serde(default)]
    pub story_share: Option<Value>,
    #[serde(default)]
    pub visual_media: Option<Value>,
    #[serde(default)]
    pub action_log: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BroadcastResponse {
    #[serde(default)]
    pub payload: Option<BroadcastPayload>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BroadcastPayload {
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub items: Vec<Item>,
}
