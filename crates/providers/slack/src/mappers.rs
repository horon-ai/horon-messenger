use horon_messenger_core::{Chat, ChatKind, Member, Message, MessagePage, ProviderKind};
use serde_json::Value;

use crate::models::{SlackChannel, SlackMessage, SlackSearchMessages, SlackUser};

const PROVIDER: ProviderKind = ProviderKind::Slack;

fn chat_kind(channel: &SlackChannel) -> ChatKind {
    if channel.is_im {
        ChatKind::Direct
    } else if channel.is_mpim {
        ChatKind::Group
    } else {
        ChatKind::Channel
    }
}

fn chat_name(channel: &SlackChannel) -> String {
    if !channel.name.is_empty() {
        channel.name.clone()
    } else if let Some(user) = &channel.user {
        user.clone()
    } else {
        channel.id.clone()
    }
}

pub fn chat(channel: &SlackChannel) -> Chat {
    Chat {
        provider: PROVIDER,
        id: channel.id.clone(),
        name: chat_name(channel),
        kind: chat_kind(channel),
        unread_count: None,
        last_message_at: None,
        member_count: channel.num_members,
        extra: Value::Null,
    }
}

pub fn message(chat_id: &str, message: &SlackMessage) -> Message {
    Message {
        provider: PROVIDER,
        id: message.ts.clone(),
        chat_id: chat_id.to_owned(),
        sender_id: message.user.clone(),
        sender_name: message.username.clone().or_else(|| message.user.clone()),
        text: message.text.clone(),
        sent_at: message.ts.clone(),
        edited_at: message.edited.as_ref().and_then(|edited| edited.ts.clone()),
        attachments: Vec::new(),
        reply_to: None,
        extra: Value::Null,
    }
}

pub fn member(user: &SlackUser) -> Member {
    let display = user
        .profile
        .as_ref()
        .and_then(|profile| profile.display_name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            user.profile
                .as_ref()
                .and_then(|profile| profile.real_name.clone())
                .filter(|name| !name.is_empty())
        })
        .or_else(|| user.real_name.clone().filter(|name| !name.is_empty()))
        .unwrap_or_else(|| user.name.clone());
    Member {
        id: user.id.clone(),
        display_name: display,
        username: Some(user.name.clone()),
        is_self: false,
        extra: Value::Null,
    }
}

pub fn search_page(chat_id: &str, response: &SlackSearchMessages) -> MessagePage {
    let matches = response
        .messages
        .as_ref()
        .map(|inner| inner.matches.as_slice())
        .unwrap_or(&[]);
    let messages = matches
        .iter()
        .map(|hit| Message {
            provider: PROVIDER,
            id: hit.ts.clone(),
            chat_id: hit
                .channel
                .as_ref()
                .and_then(|channel| channel.id.clone())
                .unwrap_or_default(),
            sender_id: hit.user.clone(),
            sender_name: hit.username.clone().or_else(|| hit.user.clone()),
            text: hit.text.clone(),
            sent_at: hit.ts.clone(),
            edited_at: None,
            attachments: Vec::new(),
            reply_to: None,
            extra: Value::Null,
        })
        .collect::<Vec<_>>();
    MessagePage {
        provider: PROVIDER,
        chat_id: chat_id.to_owned(),
        messages,
        has_more: false,
        newest_first: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_response_maps_to_a_message_page() {
        let response: SlackSearchMessages = serde_json::from_value(serde_json::json!({
            "ok": true,
            "messages": {
                "total": 1,
                "matches": [{
                    "ts": "1700000000.000100",
                    "text": "seeded hello from mock slack",
                    "user": "U2",
                    "username": "bob",
                    "channel": { "id": "C1", "name": "general" },
                    "permalink": "https://mock.slack.com/archives/C1/p1700000000000100"
                }]
            }
        }))
        .unwrap();
        let page = search_page("C1", &response);
        assert_eq!(page.provider, ProviderKind::Slack);
        assert_eq!(page.chat_id, "C1");
        assert_eq!(page.messages.len(), 1);
        let message = &page.messages[0];
        assert_eq!(message.id, "1700000000.000100");
        assert_eq!(message.chat_id, "C1");
        assert_eq!(message.sender_id.as_deref(), Some("U2"));
        assert_eq!(message.sender_name.as_deref(), Some("bob"));
        assert_eq!(message.text, "seeded hello from mock slack");
        assert!(!page.has_more);
        assert!(page.newest_first);
    }

    #[test]
    fn search_page_handles_an_empty_messages_block() {
        let response: SlackSearchMessages =
            serde_json::from_value(serde_json::json!({ "ok": true })).unwrap();
        let page = search_page("", &response);
        assert!(page.messages.is_empty());
        assert_eq!(page.chat_id, "");
    }

    #[test]
    fn chat_maps_im_and_channel_kinds() {
        let im: SlackChannel = serde_json::from_value(serde_json::json!({
            "id": "D1",
            "is_im": true,
            "user": "U9"
        }))
        .unwrap();
        let direct = chat(&im);
        assert_eq!(direct.kind, ChatKind::Direct);
        assert_eq!(direct.name, "U9");

        let channel: SlackChannel = serde_json::from_value(serde_json::json!({
            "id": "C1",
            "name": "general",
            "num_members": 5
        }))
        .unwrap();
        let mapped = chat(&channel);
        assert_eq!(mapped.kind, ChatKind::Channel);
        assert_eq!(mapped.name, "general");
        assert_eq!(mapped.member_count, Some(5));
    }
}
