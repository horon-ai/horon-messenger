use serde::Deserialize;

/// A Telegram user or bot account.
#[derive(Debug, Clone, Deserialize)]
pub struct BotUser {
    pub id: i64,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

impl BotUser {
    /// The best display name: full name, else username, else the numeric id.
    pub fn display_name(&self) -> String {
        let full = [Some(self.first_name.as_str()), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !full.is_empty() {
            return full;
        }
        self.username.clone().unwrap_or_else(|| self.id.to_string())
    }
}

/// A Telegram chat, group, supergroup or channel.
#[derive(Debug, Clone, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub member_count: Option<u64>,
}

impl TelegramChat {
    /// The best display name: title, else full name, else username, else the numeric id.
    pub fn display_name(&self) -> String {
        if let Some(title) = self.title.as_deref().filter(|title| !title.is_empty()) {
            return title.to_owned();
        }
        let full = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !full.is_empty() {
            return full;
        }
        self.username.clone().unwrap_or_else(|| self.id.to_string())
    }
}

/// A member of a chat, as returned by `getChatAdministrators`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMember {
    pub user: BotUser,
}

/// A Telegram message.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub message_id: i64,
    #[serde(default)]
    pub date: i64,
    pub chat: TelegramChat,
    #[serde(default)]
    pub from: Option<BotUser>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub reply_to_message: Option<Box<Message>>,
}

impl Message {
    /// The message text, falling back to the caption for media messages.
    pub fn body(&self) -> String {
        self.text
            .clone()
            .or_else(|| self.caption.clone())
            .unwrap_or_default()
    }
}

/// A callback query carried inside an update.
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    #[serde(default)]
    pub message: Option<Message>,
}

/// A chat member transition carried inside an update.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMemberUpdated {
    pub chat: TelegramChat,
}

/// A Telegram update, the only source of chats and messages for a bot.
#[derive(Debug, Clone, Deserialize)]
pub struct Update {
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub edited_message: Option<Message>,
    #[serde(default)]
    pub channel_post: Option<Message>,
    #[serde(default)]
    pub edited_channel_post: Option<Message>,
    #[serde(default)]
    pub callback_query: Option<CallbackQuery>,
    #[serde(default)]
    pub my_chat_member: Option<ChatMemberUpdated>,
    #[serde(default)]
    pub chat_member: Option<ChatMemberUpdated>,
}

impl Update {
    /// Every message this update carries.
    pub fn messages(&self) -> Vec<&Message> {
        let mut out: Vec<&Message> = [
            self.message.as_ref(),
            self.edited_message.as_ref(),
            self.channel_post.as_ref(),
            self.edited_channel_post.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        if let Some(message) = self
            .callback_query
            .as_ref()
            .and_then(|query| query.message.as_ref())
        {
            out.push(message);
        }
        out
    }

    /// Every chat this update references.
    pub fn chats(&self) -> Vec<&TelegramChat> {
        let mut out: Vec<&TelegramChat> = self.messages().into_iter().map(|m| &m.chat).collect();
        for updated in [self.my_chat_member.as_ref(), self.chat_member.as_ref()]
            .into_iter()
            .flatten()
        {
            out.push(&updated.chat);
        }
        out
    }
}

/// The Bot API's `{ok, result, description, error_code, parameters}` envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiEnvelope {
    pub ok: bool,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub error_code: Option<i64>,
    #[serde(default)]
    pub parameters: Option<ApiParameters>,
}

/// The optional error parameters a Bot API error carries.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiParameters {
    #[serde(default)]
    pub retry_after: Option<u64>,
}
