use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ImsgChat {
    pub(crate) id: u64,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) identifier: Option<String>,
    #[serde(default)]
    pub(crate) guid: Option<String>,
    #[serde(default)]
    pub(crate) service: Option<String>,
    #[serde(default)]
    pub(crate) is_group: bool,
    #[serde(default)]
    pub(crate) participants: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) last_message: Option<ImsgMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ImsgMessage {
    pub(crate) id: u64,
    pub(crate) chat_id: u64,
    #[serde(default)]
    pub(crate) guid: Option<String>,
    #[serde(default)]
    pub(crate) sender: Option<String>,
    #[serde(default)]
    pub(crate) sender_name: Option<String>,
    #[serde(default)]
    pub(crate) is_from_me: bool,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatsList {
    pub(crate) chats: Vec<ImsgChat>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessagesHistory {
    pub(crate) messages: Vec<ImsgMessage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SendResponse {
    #[serde(default)]
    pub(crate) ok: Option<bool>,
    #[serde(default)]
    pub(crate) guid: Option<String>,
    #[serde(default)]
    pub(crate) id: Option<u64>,
    #[serde(default)]
    pub(crate) service: Option<String>,
}
