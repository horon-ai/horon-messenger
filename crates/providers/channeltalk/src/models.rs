use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelAccount {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelManager {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageBlock {
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    #[serde(default, rename = "personId")]
    pub person_id: Option<String>,
    #[serde(default, rename = "personType")]
    pub person_type: Option<String>,
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub blocks: Vec<MessageBlock>,
    #[serde(default, rename = "plainText")]
    pub plain_text: Option<String>,
}

impl ChannelMessage {
    pub fn text(&self) -> String {
        let mut parts: Vec<String> = self
            .blocks
            .iter()
            .filter_map(|block| block.value.clone())
            .collect();
        if let Some(plain) = &self.plain_text
            && !plain.is_empty()
        {
            parts.push(plain.clone());
        }
        parts.join("\n")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchHit {
    pub source: ChannelMessage,
}
