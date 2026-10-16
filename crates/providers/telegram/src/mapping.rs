//! Pure TDLib JSON to domain mapping, shared by the real and `qa-fake` backends.

use horon_messenger_core::{Attachment, Chat, ChatKind, Error, Member, Message, Profile, ProviderKind, Result};
use serde_json::Value;

const PROVIDER: ProviderKind = ProviderKind::Telegram;

fn protocol(message: impl Into<String>) -> Error {
    Error::Protocol {
        provider: PROVIDER,
        message: message.into(),
    }
}

fn as_i64(value: &Value, name: &str) -> Result<i64> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| protocol(format!("`{name}` must be an integer")))
}

/// Parses a raw TDLib response string, mapping non-JSON output to `Protocol`.
pub fn parse_response(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).map_err(|error| protocol(format!("TDLib returned non-JSON output: {error}")))
}

/// Maps a TDLib `user` object to a [`Profile`].
pub fn profile(user: &Value) -> Result<Profile> {
    let id = as_i64(user, "id")?;
    Ok(Profile {
        provider: PROVIDER,
        account_id: id.to_string(),
        display_name: display_name(user),
        username: username(user),
        extra: Value::Null,
    })
}

/// Maps a TDLib `chat` object to a [`Chat`].
pub fn chat(value: &Value) -> Result<Chat> {
    let id = as_i64(value, "id")?;
    Ok(Chat {
        provider: PROVIDER,
        id: id.to_string(),
        name: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        kind: value.get("type").map(chat_kind).unwrap_or(ChatKind::Unknown),
        unread_count: value.get("unread_count").and_then(Value::as_u64),
        last_message_at: value
            .get("last_message")
            .and_then(|message| message.get("date"))
            .and_then(Value::as_i64)
            .map(unix_to_rfc3339),
        member_count: value
            .get("type")
            .and_then(|kind| kind.get("member_count"))
            .and_then(Value::as_u64),
        extra: Value::Null,
    })
}

/// Maps a TDLib `message` object to a [`Message`].
pub fn message(value: &Value, fallback_chat_id: i64) -> Result<Message> {
    let id = as_i64(value, "id")?;
    let chat_id = value.get("chat_id").and_then(Value::as_i64).unwrap_or(fallback_chat_id);
    let content = value.get("content");
    Ok(Message {
        provider: PROVIDER,
        id: id.to_string(),
        chat_id: chat_id.to_string(),
        sender_id: value.get("sender_id").and_then(sender_id),
        sender_name: None,
        text: content.map(extract_text).unwrap_or_default(),
        sent_at: value
            .get("date")
            .and_then(Value::as_i64)
            .map(unix_to_rfc3339)
            .unwrap_or_else(horon_messenger_core::now_rfc3339),
        edited_at: value.get("edit_date").and_then(Value::as_i64).map(unix_to_rfc3339),
        attachments: content.map(attachments).unwrap_or_default(),
        reply_to: value
            .get("reply_to")
            .and_then(|reply| reply.get("message_id"))
            .and_then(Value::as_i64)
            .map(|id| id.to_string()),
        extra: Value::Null,
    })
}

/// Maps a TDLib `user` object to a [`Member`].
pub fn member(user: &Value, self_id: Option<i64>) -> Result<Member> {
    let id = as_i64(user, "id")?;
    Ok(Member {
        id: id.to_string(),
        display_name: display_name(user).unwrap_or_else(|| id.to_string()),
        username: username(user),
        is_self: self_id == Some(id),
        extra: Value::Null,
    })
}

/// The authorization state summarized from a TDLib `AuthorizationState` object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationSummary {
    pub state: String,
    pub authenticated: bool,
    pub next_action: Option<String>,
}

/// Summarizes a TDLib authorization state into the fields the auth surface needs.
pub fn authorization_state(value: &Value) -> AuthorizationSummary {
    let state = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("authorizationStateUnknown")
        .to_owned();
    let (authenticated, next_action) = match state.as_str() {
        "authorizationStateReady" => (true, None),
        "authorizationStateWaitTdlibParameters" => (false, Some("provide_api_credentials")),
        "authorizationStateWaitPhoneNumber" => (false, Some("provide_phone_number")),
        "authorizationStateWaitCode" => (false, Some("provide_code")),
        "authorizationStateWaitPassword" => (false, Some("provide_password")),
        "authorizationStateWaitEmailAddress" => (false, Some("provide_email")),
        "authorizationStateWaitEmailCode" => (false, Some("provide_email_code")),
        "authorizationStateWaitRegistration" => (false, Some("provide_registration")),
        "authorizationStateWaitPremiumPurchase" => (false, Some("complete_premium_purchase")),
        _ => (false, None),
    };
    AuthorizationSummary {
        state,
        authenticated,
        next_action: next_action.map(str::to_owned),
    }
}

/// Reads the `chat_ids` array from a TDLib `chats` object.
pub fn chat_ids(value: &Value) -> Result<Vec<i64>> {
    value
        .get("chat_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .ok_or_else(|| protocol("`chat_ids` must be an array"))
}

/// Formats Unix seconds as an RFC 3339 UTC string.
pub fn unix_to_rfc3339(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(horon_messenger_core::now_rfc3339)
}

fn display_name(user: &Value) -> Option<String> {
    let first = user.get("first_name").and_then(Value::as_str).unwrap_or_default();
    let last = user.get("last_name").and_then(Value::as_str).unwrap_or_default();
    let joined = format!("{first} {last}").trim().to_owned();
    (!joined.is_empty()).then_some(joined)
}

fn username(user: &Value) -> Option<String> {
    let usernames = user.get("usernames");
    usernames
        .and_then(|value| value.get("editable_username"))
        .and_then(Value::as_str)
        .or_else(|| {
            usernames
                .and_then(|value| value.get("active_usernames"))
                .and_then(Value::as_array)
                .and_then(|names| names.first())
                .and_then(Value::as_str)
        })
        .or_else(|| user.get("username").and_then(Value::as_str))
        .map(str::to_owned)
        .filter(|name| !name.is_empty())
}

fn chat_kind(kind: &Value) -> ChatKind {
    match kind.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") | Some("chatTypeSecret") => ChatKind::Direct,
        Some("chatTypeBasicGroup") => ChatKind::Group,
        Some("chatTypeSupergroup") => {
            if kind.get("is_channel").and_then(Value::as_bool).unwrap_or(false) {
                ChatKind::Channel
            } else {
                ChatKind::Group
            }
        }
        _ => ChatKind::Unknown,
    }
}

fn sender_id(sender: &Value) -> Option<String> {
    match sender.get("@type").and_then(Value::as_str) {
        Some("messageSenderUser") => sender.get("user_id").and_then(Value::as_i64).map(|id| id.to_string()),
        Some("messageSenderChat") => sender.get("chat_id").and_then(Value::as_i64).map(|id| id.to_string()),
        _ => None,
    }
}

fn extract_text(content: &Value) -> String {
    match content.get("@type").and_then(Value::as_str) {
        Some("messageText") => formatted_text(content.get("text")),
        Some("messagePhoto")
        | Some("messageVideo")
        | Some("messageAnimation")
        | Some("messageAudio")
        | Some("messageDocument")
        | Some("messageVoiceNote") => formatted_text(content.get("caption")),
        Some("messagePoll") => content
            .get("poll")
            .and_then(|poll| poll.get("question"))
            .map(|question| formatted_text(Some(question)))
            .unwrap_or_default(),
        Some("messageSticker") => content
            .get("sticker")
            .and_then(|sticker| sticker.get("emoji"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Some("messageContact") => contact_name(content.get("contact")),
        _ => String::new(),
    }
}

fn formatted_text(value: Option<&Value>) -> String {
    value
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn contact_name(contact: Option<&Value>) -> String {
    let Some(contact) = contact else {
        return String::new();
    };
    let first = contact.get("first_name").and_then(Value::as_str).unwrap_or_default();
    let last = contact.get("last_name").and_then(Value::as_str).unwrap_or_default();
    format!("{first} {last}").trim().to_owned()
}

fn attachments(content: &Value) -> Vec<Attachment> {
    match content.get("@type").and_then(Value::as_str) {
        Some("messagePhoto")
        | Some("messageVideo")
        | Some("messageAnimation")
        | Some("messageAudio")
        | Some("messageDocument")
        | Some("messageVoiceNote") => find_file(content).map(attachment).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn find_file(content: &Value) -> Option<&Value> {
    for key in ["document", "video", "audio", "voice_note", "animation", "photo"] {
        let Some(holder) = content.get(key) else {
            continue;
        };
        if let Some(file) = ["document", "video", "audio", "voice_note", "animation"]
            .iter()
            .find_map(|inner| holder.get(inner))
        {
            return Some(file);
        }
        if holder.get("id").is_some() || holder.get("file_name").is_some() {
            return Some(holder);
        }
        if let Some(photo) = holder
            .get("sizes")
            .and_then(Value::as_array)
            .and_then(|sizes| sizes.last())
            .and_then(|size| size.get("photo"))
        {
            return Some(photo);
        }
    }
    None
}

fn attachment(file: &Value) -> Attachment {
    Attachment {
        id: file.get("id").and_then(Value::as_i64).map(|id| id.to_string()),
        file_name: non_empty(file.get("file_name").and_then(Value::as_str)),
        mime_type: non_empty(file.get("mime_type").and_then(Value::as_str)),
        size_bytes: file.get("size").and_then(Value::as_u64),
        url: None,
        local_path: non_empty(
            file.get("local")
                .and_then(|local| local.get("path"))
                .and_then(Value::as_str),
        ),
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|text| !text.is_empty()).map(str::to_owned)
}
