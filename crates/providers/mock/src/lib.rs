use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatKind, ChatQuery,
    Delivery, Error, LoginOptions, LoginOutcome, Member, Message, MessagePage, MessageQuery,
    Profile, Provider, ProviderKind, Result, SendRequest, SendResult, Snapshot, now_rfc3339,
};

const ACCOUNT_ID: &str = "mock-user";

/// A provider that answers every operation from memory, so the whole stack can
/// be exercised without a real platform.
#[derive(Debug, Clone)]
pub struct MockProvider {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    sent: Vec<Message>,
    next_id: u64,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                sent: Vec::new(),
                next_id: 1,
            })),
        }
    }

    fn chats() -> Vec<Chat> {
        vec![
            Chat {
                provider: ProviderKind::Mock,
                id: "general".to_owned(),
                name: "#general".to_owned(),
                kind: ChatKind::Channel,
                unread_count: Some(2),
                last_message_at: Some(now_rfc3339()),
                member_count: Some(12),
                extra: Value::Null,
            },
            Chat {
                provider: ProviderKind::Mock,
                id: "team".to_owned(),
                name: "Team Standup".to_owned(),
                kind: ChatKind::Group,
                unread_count: Some(0),
                last_message_at: Some(now_rfc3339()),
                member_count: Some(5),
                extra: Value::Null,
            },
            Chat {
                provider: ProviderKind::Mock,
                id: "dm-alice".to_owned(),
                name: "Alice".to_owned(),
                kind: ChatKind::Direct,
                unread_count: None,
                last_message_at: Some(now_rfc3339()),
                member_count: Some(2),
                extra: Value::Null,
            },
        ]
    }

    fn seed(chat_id: &str) -> Vec<Message> {
        let samples: &[(&str, &str)] = match chat_id {
            "general" => &[
                ("Alice", "Welcome to the mock provider."),
                ("Bob", "Every call is answered from memory."),
            ],
            "team" => &[
                ("Carol", "Standup in five minutes."),
                ("Dave", "The scaffold lands today."),
            ],
            "dm-alice" => &[("Alice", "Ping me when the CLI works.")],
            _ => &[("Mock", "No seeded messages for this chat.")],
        };
        samples
            .iter()
            .enumerate()
            .map(|(index, (sender, text))| Message {
                provider: ProviderKind::Mock,
                id: format!("{chat_id}-{}", index + 1),
                chat_id: chat_id.to_owned(),
                sender_id: Some(sender.to_lowercase()),
                sender_name: Some((*sender).to_owned()),
                text: (*text).to_owned(),
                sent_at: now_rfc3339(),
                edited_at: None,
                attachments: Vec::new(),
                reply_to: None,
                extra: Value::Null,
            })
            .collect()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEARCH_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        Ok(Profile {
            provider: ProviderKind::Mock,
            account_id: ACCOUNT_ID.to_owned(),
            display_name: Some("Mock User".to_owned()),
            username: Some("mock".to_owned()),
            extra: Value::Null,
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let mut chats = Self::chats();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| {
                chat.name.to_lowercase().contains(&needle)
                    || chat.id.to_lowercase().contains(&needle)
            });
        }
        if let Some(limit) = query.limit {
            chats.truncate(limit);
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        Self::chats()
            .into_iter()
            .find(|chat| chat.id == chat_id)
            .ok_or_else(|| Error::NotFound {
                provider: ProviderKind::Mock,
                resource: format!("chat {chat_id}"),
            })
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let mut messages = Self::seed(chat_id);
        {
            let state = self.state.lock().expect("mock state lock");
            messages.extend(
                state
                    .sent
                    .iter()
                    .filter(|message| message.chat_id == chat_id)
                    .cloned(),
            );
        }
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            messages.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        let has_more = query.limit.is_some_and(|limit| messages.len() > limit);
        if let Some(limit) = query.limit {
            messages.truncate(limit);
        }
        Ok(MessagePage {
            provider: ProviderKind::Mock,
            chat_id: chat_id.to_owned(),
            messages,
            has_more,
            newest_first: true,
        })
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        self.list_messages(chat_id.unwrap_or("general"), query)
            .await
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let (id, sent_at) = {
            let mut state = self.state.lock().expect("mock state lock");
            let id = format!("mock-{}", state.next_id);
            state.next_id += 1;
            let sent_at = now_rfc3339();
            state.sent.push(Message {
                provider: ProviderKind::Mock,
                id: id.clone(),
                chat_id: request.chat_id.clone(),
                sender_id: Some(ACCOUNT_ID.to_owned()),
                sender_name: Some("Mock User".to_owned()),
                text: request.text.clone(),
                sent_at: sent_at.clone(),
                edited_at: None,
                attachments: request.attachments.clone(),
                reply_to: request.reply_to.clone(),
                extra: Value::Null,
            });
            (id, sent_at)
        };
        Ok(SendResult {
            provider: ProviderKind::Mock,
            chat_id: request.chat_id.clone(),
            message_id: Some(id),
            sent_at,
            accepted: true,
            delivery: Delivery::Confirmed,
            extra: Value::Null,
        })
    }

    async fn list_members(&self, _chat_id: &str) -> Result<Vec<Member>> {
        Ok(vec![
            Member {
                id: ACCOUNT_ID.to_owned(),
                display_name: "Mock User".to_owned(),
                username: Some("mock".to_owned()),
                is_self: true,
                extra: Value::Null,
            },
            Member {
                id: "alice".to_owned(),
                display_name: "Alice".to_owned(),
                username: Some("alice".to_owned()),
                is_self: false,
                extra: Value::Null,
            },
        ])
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            provider: ProviderKind::Mock,
            profile: Some(self.whoami().await?),
            chats: Self::chats(),
            generated_at: now_rfc3339(),
        })
    }
}

#[async_trait]
impl AuthProvider for MockProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Extract, AuthMethod::Login]
    }

    async fn status(&self) -> Result<AuthStatus> {
        Ok(AuthStatus {
            provider: ProviderKind::Mock,
            authenticated: true,
            current: Some(ACCOUNT_ID.to_owned()),
            accounts: vec![AccountSummary {
                account_id: ACCOUNT_ID.to_owned(),
                display_name: Some("Mock User".to_owned()),
                method: Some(AuthMethod::Extract),
                current: true,
                extra: Value::Null,
            }],
        })
    }

    async fn login(&self, _options: &LoginOptions) -> Result<LoginOutcome> {
        Ok(LoginOutcome {
            provider: ProviderKind::Mock,
            authenticated: true,
            account_id: Some(ACCOUNT_ID.to_owned()),
            next_action: None,
            message: Some("the mock provider is always authenticated".to_owned()),
        })
    }

    async fn logout(&self, _account_id: Option<&str>) -> Result<()> {
        Ok(())
    }
}
