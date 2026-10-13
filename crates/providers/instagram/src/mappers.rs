//! Maps Instagram API shapes onto the shared domain model.

use hyper_messenger_core::{Attachment, Chat, ChatKind, Member, Message, Profile, ProviderKind};
use serde_json::Value;

use crate::models::{IgUser, Item, Thread};

const PROVIDER: ProviderKind = ProviderKind::Instagram;

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn micros_to_rfc3339(micros: i64) -> String {
    chrono::DateTime::from_timestamp_millis(micros / 1000)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(hyper_messenger_core::now_rfc3339)
}

fn message_text(item: &Item) -> String {
    if let Some(text) = &item.text {
        return text.clone();
    }
    if let Some(text) = item
        .link
        .as_ref()
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
    {
        return text.to_owned();
    }
    if let Some(text) = item
        .reel_share
        .as_ref()
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
    {
        return text.to_owned();
    }
    if item.item_type.as_deref() == Some("reel_share") {
        return "Shared a reel".to_owned();
    }
    if let Some(text) = item
        .story_share
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
    {
        return text.to_owned();
    }
    if let Some(text) = item
        .media_share
        .as_ref()
        .and_then(|value| value.get("caption"))
        .and_then(|caption| caption.get("text"))
        .and_then(Value::as_str)
    {
        return text.to_owned();
    }
    if item.item_type.as_deref() == Some("like") {
        return "Liked a message".to_owned();
    }
    if let Some(text) = item
        .action_log
        .as_ref()
        .and_then(|value| value.get("description"))
        .and_then(Value::as_str)
    {
        return text.to_owned();
    }
    String::new()
}

fn image_candidate(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| value.get("image_versions2"))
        .and_then(|value| value.get("candidates"))
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|first| first.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn media_url(item: &Item) -> Option<String> {
    if let Some(url) = image_candidate(item.media.as_ref()) {
        return Some(url);
    }
    if let Some(url) = item
        .media
        .as_ref()
        .and_then(|value| value.get("video_versions"))
        .and_then(Value::as_array)
        .and_then(|versions| versions.first())
        .and_then(|first| first.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        return Some(url);
    }
    if let Some(url) = image_candidate(item.media_share.as_ref()) {
        return Some(url);
    }
    image_candidate(
        item.visual_media
            .as_ref()
            .and_then(|value| value.get("media")),
    )
}

fn thread_name(thread: &Thread, fallback: &str) -> String {
    if let Some(title) = thread
        .thread_title
        .as_ref()
        .filter(|title| !title.is_empty())
    {
        return title.clone();
    }
    let joined = thread
        .users
        .iter()
        .map(|user| {
            user.full_name
                .clone()
                .or_else(|| user.username.clone())
                .unwrap_or_default()
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        fallback.to_owned()
    } else {
        joined
    }
}

/// Maps an Instagram thread onto the domain chat.
pub fn chat(thread: &Thread) -> Chat {
    let id = thread.thread_id.clone().unwrap_or_default();
    let name = thread_name(thread, &id);
    let is_group = thread.thread_type.as_deref() == Some("group") || thread.users.len() > 1;
    let last_message_at = thread
        .items
        .first()
        .and_then(|item| item.timestamp)
        .map(micros_to_rfc3339);
    Chat {
        provider: PROVIDER,
        id,
        name,
        kind: if is_group {
            ChatKind::Group
        } else {
            ChatKind::Direct
        },
        unread_count: Some(thread.read_state.unwrap_or(0)),
        last_message_at,
        member_count: Some(thread.users.len() as u64 + 1),
        extra: Value::Null,
    }
}

/// Maps an Instagram item onto the domain message.
pub fn message(item: &Item, chat_id: &str) -> Message {
    let attachments = media_url(item)
        .map(|url| {
            vec![Attachment {
                id: None,
                file_name: None,
                mime_type: None,
                size_bytes: None,
                url: Some(url),
                local_path: None,
            }]
        })
        .unwrap_or_default();
    Message {
        provider: PROVIDER,
        id: item.item_id.clone().unwrap_or_default(),
        chat_id: chat_id.to_owned(),
        sender_id: item
            .user_id
            .as_ref()
            .map(value_to_string)
            .filter(|id| !id.is_empty()),
        sender_name: None,
        text: message_text(item),
        sent_at: item
            .timestamp
            .map(micros_to_rfc3339)
            .unwrap_or_else(hyper_messenger_core::now_rfc3339),
        edited_at: None,
        attachments,
        reply_to: None,
        extra: Value::Null,
    }
}

/// Maps an Instagram user onto the domain member.
pub fn member(user: &IgUser, viewer_id: &str) -> Member {
    let id = user.pk.as_ref().map(value_to_string).unwrap_or_default();
    let display_name = user
        .full_name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| user.username.clone())
        .unwrap_or_else(|| id.clone());
    Member {
        id: id.clone(),
        display_name,
        username: user.username.clone(),
        is_self: id == viewer_id,
        extra: Value::Null,
    }
}

/// Maps an Instagram user onto the domain profile.
pub fn profile(user: &IgUser) -> Profile {
    Profile {
        provider: PROVIDER,
        account_id: user.pk.as_ref().map(value_to_string).unwrap_or_default(),
        display_name: user.full_name.clone().filter(|name| !name.is_empty()),
        username: user.username.clone(),
        extra: match &user.profile_pic_url {
            Some(url) => serde_json::json!({ "profile_pic_url": url }),
            None => Value::Null,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(value: Value) -> Item {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn thread_maps_to_a_direct_chat_with_a_last_message_time() {
        let thread: Thread = serde_json::from_value(serde_json::json!({
            "thread_id": "T1",
            "thread_type": "private",
            "read_state": 3,
            "users": [{ "pk": 7, "username": "bob", "full_name": "Bob Lee" }],
            "items": [{ "item_id": "i1", "user_id": 7, "timestamp": 1700000060000000_i64, "item_type": "text", "text": "latest" }]
        }))
        .unwrap();
        let chat = chat(&thread);
        assert_eq!(chat.provider, ProviderKind::Instagram);
        assert_eq!(chat.id, "T1");
        assert_eq!(chat.name, "Bob Lee");
        assert_eq!(chat.kind, ChatKind::Direct);
        assert_eq!(chat.unread_count, Some(3));
        assert_eq!(chat.member_count, Some(2));
        assert_eq!(
            chat.last_message_at.as_deref(),
            Some("2023-11-14T22:14:20.000Z")
        );
    }

    #[test]
    fn thread_with_a_title_and_multiple_users_is_a_group() {
        let thread: Thread = serde_json::from_value(serde_json::json!({
            "thread_id": "G1",
            "thread_title": "Team",
            "thread_type": "group",
            "users": [
                { "pk": 7, "username": "bob" },
                { "pk": 8, "username": "carol", "full_name": "Carol" }
            ]
        }))
        .unwrap();
        let chat = chat(&thread);
        assert_eq!(chat.name, "Team");
        assert_eq!(chat.kind, ChatKind::Group);
        assert_eq!(chat.member_count, Some(3));
        assert_eq!(chat.unread_count, Some(0));
    }

    #[test]
    fn text_item_maps_id_text_sender_and_timestamp() {
        let value = serde_json::json!({
            "item_id": "ig-older-1",
            "user_id": 7,
            "timestamp": 1700000000000000_i64,
            "item_type": "text",
            "text": "seeded hello from mock instagram"
        });
        let message = message(&item(value), "T1");
        assert_eq!(message.id, "ig-older-1");
        assert_eq!(message.chat_id, "T1");
        assert_eq!(message.sender_id.as_deref(), Some("7"));
        assert_eq!(message.text, "seeded hello from mock instagram");
        assert_eq!(message.sent_at, "2023-11-14T22:13:20.000Z");
        assert!(message.attachments.is_empty());
    }

    #[test]
    fn media_item_maps_an_image_attachment_url() {
        let value = serde_json::json!({
            "item_id": "m1",
            "item_type": "media",
            "media": {
                "image_versions2": {
                    "candidates": [{ "url": "https://cdn.example/a.jpg", "width": 100 }]
                }
            }
        });
        let message = message(&item(value), "T1");
        assert_eq!(message.attachments.len(), 1);
        assert_eq!(
            message.attachments[0].url.as_deref(),
            Some("https://cdn.example/a.jpg")
        );
    }

    #[test]
    fn like_item_maps_to_a_placeholder_text() {
        let message = message(
            &item(serde_json::json!({ "item_id": "l1", "item_type": "like" })),
            "T1",
        );
        assert_eq!(message.text, "Liked a message");
    }

    #[test]
    fn member_marks_self_and_profile_maps_the_account() {
        let user: IgUser = serde_json::from_value(serde_json::json!({
            "pk": 4242,
            "username": "alice",
            "full_name": "Alice Doe",
            "profile_pic_url": "https://cdn.example/alice.jpg"
        }))
        .unwrap();
        let member = member(&user, "4242");
        assert_eq!(member.id, "4242");
        assert_eq!(member.display_name, "Alice Doe");
        assert!(member.is_self);
        let profile = profile(&user);
        assert_eq!(profile.account_id, "4242");
        assert_eq!(profile.username.as_deref(), Some("alice"));
        assert_eq!(profile.display_name.as_deref(), Some("Alice Doe"));
        assert_eq!(
            profile.extra["profile_pic_url"].as_str(),
            Some("https://cdn.example/alice.jpg")
        );
    }
}
