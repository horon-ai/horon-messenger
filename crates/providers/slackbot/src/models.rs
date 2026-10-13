use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AuthTest {
    pub user_id: String,
    pub team_id: String,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub team: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub num_members: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelList {
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelInfo {
    pub channel: Channel,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMessage {
    pub ts: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thread_ts: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct History {
    pub messages: Vec<RawMessage>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostedMessage {
    pub ts: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemberList {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub user: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub profile: Option<UserProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
}

impl User {
    pub fn display_name(&self) -> String {
        self.profile
            .as_ref()
            .and_then(|profile| profile.display_name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| self.profile.as_ref().and_then(|p| p.real_name.clone()))
            .filter(|name| !name.is_empty())
            .or_else(|| self.real_name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| self.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.id.clone())
    }
}
