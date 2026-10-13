use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LineChat {
    pub(crate) chat_id: String,
    #[serde(rename = "type", default)]
    pub(crate) chat_type: String,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) member_count: Option<u64>,
    #[serde(default)]
    pub(crate) picture_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LineMessage {
    pub(crate) message_id: String,
    #[serde(default)]
    pub(crate) chat_id: String,
    #[serde(default)]
    pub(crate) author_id: String,
    #[serde(default)]
    pub(crate) author_name: Option<String>,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) content_type: Option<String>,
    #[serde(default)]
    pub(crate) sent_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LineMember {
    pub(crate) mid: String,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LineProfile {
    pub(crate) mid: String,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) status_message: Option<String>,
    #[serde(default)]
    pub(crate) picture_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LineSendResult {
    #[serde(default)]
    pub(crate) success: bool,
    #[serde(default)]
    pub(crate) chat_id: String,
    #[serde(default)]
    pub(crate) message_id: Option<String>,
    #[serde(default)]
    pub(crate) sent_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LoginResponse {
    #[serde(default)]
    pub(crate) authenticated: bool,
    #[serde(default)]
    pub(crate) account_id: Option<String>,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) device: Option<String>,
    #[serde(default)]
    pub(crate) auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatsList {
    pub(crate) chats: Vec<LineChat>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessagesList {
    pub(crate) messages: Vec<LineMessage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MembersList {
    pub(crate) members: Vec<LineMember>,
}
