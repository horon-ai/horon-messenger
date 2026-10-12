use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordAuthor {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub author: DiscordAuthor,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordGuildMember {
    pub user: DiscordUser,
    #[serde(default)]
    pub nick: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordSearchHit {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub content: String,
    pub author: DiscordAuthor,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub hit: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordSearchResponse {
    pub messages: Vec<Vec<DiscordSearchHit>>,
}
