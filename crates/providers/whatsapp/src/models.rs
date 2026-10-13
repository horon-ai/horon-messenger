//! Wire types for the sidecar protocol and their mapping to the core model.

use serde::Deserialize;
use serde_json::{Map, Value};

use hyper_messenger_core::{Chat, ChatKind, Member, Message, ProviderKind, now_rfc3339};

const PROVIDER: ProviderKind = ProviderKind::WhatsApp;

/// A chat as the sidecar reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarChat {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub chat_type: Option<String>,
    #[serde(default)]
    pub unread_count: Option<u64>,
    #[serde(default)]
    pub last_message: Option<SidecarMessage>,
    #[serde(default)]
    pub members: Vec<SidecarMember>,
}

/// A message as the sidecar reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarMessage {
    pub id: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub from_name: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub is_outgoing: bool,
    #[serde(default, rename = "type")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

/// A chat member as the sidecar reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarMember {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// The account the sidecar is linked to.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarAccount {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
}

/// The result of an `auth.status` request.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarAuthStatus {
    pub authenticated: bool,
    #[serde(default)]
    pub account: Option<SidecarAccount>,
}

/// The result of an `auth.login` request.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarLogin {
    pub authenticated: bool,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value.clone().filter(|text| !text.is_empty())
}

/// Maps a sidecar chat type to a core chat kind.
pub fn chat_kind(chat_type: Option<&str>) -> ChatKind {
    match chat_type {
        Some("individual") => ChatKind::Direct,
        Some("group") => ChatKind::Group,
        Some("broadcast") | Some("status") => ChatKind::Channel,
        _ => ChatKind::Unknown,
    }
}

/// Maps a sidecar chat to a core chat.
pub fn chat_from_sidecar(chat: &SidecarChat) -> Chat {
    Chat {
        provider: PROVIDER,
        id: chat.id.clone(),
        name: non_empty(&chat.name).unwrap_or_else(|| chat.id.clone()),
        kind: chat_kind(chat.chat_type.as_deref()),
        unread_count: chat.unread_count,
        last_message_at: chat
            .last_message
            .as_ref()
            .and_then(|message| non_empty(&message.timestamp)),
        member_count: if chat.members.is_empty() {
            None
        } else {
            Some(chat.members.len() as u64)
        },
        extra: Value::Null,
    }
}

/// Maps a sidecar message to a core message.
pub fn message_from_sidecar(message: &SidecarMessage) -> Message {
    let mut extra = Map::new();
    extra.insert(
        "type".to_owned(),
        message
            .message_type
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    extra.insert("is_outgoing".to_owned(), Value::Bool(message.is_outgoing));
    Message {
        provider: PROVIDER,
        id: message.id.clone(),
        chat_id: message.chat_id.clone(),
        sender_id: non_empty(&message.from),
        sender_name: non_empty(&message.from_name),
        text: message.text.clone().unwrap_or_default(),
        sent_at: non_empty(&message.timestamp).unwrap_or_else(now_rfc3339),
        edited_at: None,
        attachments: Vec::new(),
        reply_to: None,
        extra: Value::Object(extra),
    }
}

/// Maps a sidecar member to a core member.
pub fn member_from_sidecar(member: &SidecarMember) -> Member {
    Member {
        id: member.id.clone(),
        display_name: non_empty(&member.name).unwrap_or_else(|| member.id.clone()),
        username: None,
        is_self: false,
        extra: Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(value: Value) -> Message {
        message_from_sidecar(&serde_json::from_value(value).unwrap())
    }

    #[test]
    fn maps_a_reference_incoming_message() {
        let mapped = message(serde_json::json!({
            "id": "msg-1",
            "chat_id": "12025551234@s.whatsapp.net",
            "from": "12025551234@s.whatsapp.net",
            "from_name": "Alice",
            "timestamp": "2023-11-14T22:13:20.000Z",
            "is_outgoing": false,
            "type": "text",
            "text": "Hello"
        }));
        assert_eq!(mapped.provider, ProviderKind::WhatsApp);
        assert_eq!(mapped.id, "msg-1");
        assert_eq!(mapped.chat_id, "12025551234@s.whatsapp.net");
        assert_eq!(
            mapped.sender_id.as_deref(),
            Some("12025551234@s.whatsapp.net")
        );
        assert_eq!(mapped.sender_name.as_deref(), Some("Alice"));
        assert_eq!(mapped.text, "Hello");
        assert_eq!(mapped.sent_at, "2023-11-14T22:13:20.000Z");
        assert_eq!(mapped.extra["type"], "text");
        assert_eq!(mapped.extra["is_outgoing"], false);
    }

    #[test]
    fn maps_a_reference_outgoing_message_without_text() {
        let mapped = message(serde_json::json!({
            "id": "msg-2",
            "chat_id": "1234-5678@g.us",
            "from": "",
            "timestamp": "2023-11-14T22:14:00.000Z",
            "is_outgoing": true,
            "type": "image"
        }));
        assert_eq!(mapped.sender_id, None);
        assert_eq!(mapped.text, "");
        assert_eq!(mapped.extra["type"], "image");
        assert_eq!(mapped.extra["is_outgoing"], true);
    }

    #[test]
    fn maps_a_reference_individual_chat() {
        let chat: SidecarChat = serde_json::from_value(serde_json::json!({
            "id": "12025551234@s.whatsapp.net",
            "name": "Alice",
            "type": "individual",
            "unread_count": 2,
            "last_message": {
                "id": "msg-9",
                "chat_id": "12025551234@s.whatsapp.net",
                "timestamp": "2023-11-14T22:13:20.000Z",
                "is_outgoing": false,
                "type": "text",
                "text": "hi"
            }
        }))
        .unwrap();
        let mapped = chat_from_sidecar(&chat);
        assert_eq!(mapped.kind, ChatKind::Direct);
        assert_eq!(mapped.name, "Alice");
        assert_eq!(mapped.unread_count, Some(2));
        assert_eq!(
            mapped.last_message_at.as_deref(),
            Some("2023-11-14T22:13:20.000Z")
        );
        assert_eq!(mapped.member_count, None);
    }

    #[test]
    fn maps_a_reference_group_chat_with_members() {
        let chat: SidecarChat = serde_json::from_value(serde_json::json!({
            "id": "1234-5678@g.us",
            "type": "group",
            "members": [{ "id": "a@s.whatsapp.net" }, { "id": "b@s.whatsapp.net", "name": "Bob" }]
        }))
        .unwrap();
        let mapped = chat_from_sidecar(&chat);
        assert_eq!(mapped.kind, ChatKind::Group);
        assert_eq!(mapped.name, "1234-5678@g.us");
        assert_eq!(mapped.member_count, Some(2));
        let members: Vec<Member> = chat.members.iter().map(member_from_sidecar).collect();
        assert_eq!(members[0].display_name, "a@s.whatsapp.net");
        assert_eq!(members[1].display_name, "Bob");
    }

    #[test]
    fn maps_chat_types_to_core_kinds() {
        assert_eq!(chat_kind(Some("individual")), ChatKind::Direct);
        assert_eq!(chat_kind(Some("group")), ChatKind::Group);
        assert_eq!(chat_kind(Some("broadcast")), ChatKind::Channel);
        assert_eq!(chat_kind(Some("status")), ChatKind::Channel);
        assert_eq!(chat_kind(None), ChatKind::Unknown);
        assert_eq!(chat_kind(Some("mystery")), ChatKind::Unknown);
    }
}
