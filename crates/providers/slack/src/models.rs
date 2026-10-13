use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SlackAuthTest {
    pub user_id: String,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackChannel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_im: bool,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(default)]
    pub num_members: Option<u64>,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackResponseMetadata {
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackConversationsList {
    #[serde(default)]
    pub channels: Vec<SlackChannel>,
    #[serde(default)]
    pub response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackEdited {
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackMessage {
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub edited: Option<SlackEdited>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackHistory {
    #[serde(default)]
    pub messages: Vec<SlackMessage>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackPostMessage {
    pub ts: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackMembers {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackUserProfile {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackUser {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub profile: Option<SlackUserProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackSearchChannel {
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackSearchMatch {
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub channel: Option<SlackSearchChannel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackSearchMessagesInner {
    #[serde(default)]
    pub matches: Vec<SlackSearchMatch>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackSearchMessages {
    #[serde(default)]
    pub messages: Option<SlackSearchMessagesInner>,
}
