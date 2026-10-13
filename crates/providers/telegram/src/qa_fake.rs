//! An in-crate fake backend that replays fixture TDLib JSON through the real mapping.

use async_trait::async_trait;
use hyper_messenger_core::{Chat, Error, Member, Message, Profile, ProviderKind, Result};
use serde_json::Value;

use crate::mapping;
use crate::transport::TelegramBackend;

const PROVIDER: ProviderKind = ProviderKind::Telegram;
const STATE_ENV: &str = "HYPER_MESSENGER_TELEGRAM_FAKE_STATE";
const MALFORMED_ENV: &str = "HYPER_MESSENGER_TELEGRAM_FAKE_MALFORMED";

/// A backend that reads fixture files and maps them exactly like the real TDLib path.
pub struct QaFakeBackend {
    fixtures: std::path::PathBuf,
}

impl Default for QaFakeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl QaFakeBackend {
    /// Builds a fake rooted at the crate's `tests/fixtures` directory.
    pub fn new() -> Self {
        Self {
            fixtures: std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        }
    }

    fn read(&self, name: &str) -> Result<Value> {
        let path = self.fixtures.join(name);
        let raw = std::fs::read_to_string(&path).map_err(|error| Error::Config {
            message: format!("qa-fake fixture {} is unreadable: {error}", path.display()),
        })?;
        mapping::parse_response(&raw)
    }

    fn closed(&self) -> bool {
        std::env::var(STATE_ENV)
            .map(|value| value == "closed")
            .unwrap_or(false)
    }

    fn malformed(&self) -> bool {
        std::env::var(MALFORMED_ENV)
            .map(|value| value == "1")
            .unwrap_or(false)
    }

    fn ready(&self) -> Result<()> {
        if self.closed() {
            return Err(Error::AuthRequired { provider: PROVIDER });
        }
        Ok(())
    }

    fn chat_ids(&self) -> Result<Vec<i64>> {
        mapping::chat_ids(&self.read("chats.json")?)
    }

    fn load_chat(&self, id: i64) -> Result<Chat> {
        mapping::chat(&self.read(&format!("chat_{id}.json"))?)
    }
}

#[async_trait]
impl TelegramBackend for QaFakeBackend {
    async fn whoami(&self) -> Result<Profile> {
        self.ready()?;
        let name = if self.malformed() {
            "malformed.json"
        } else {
            "get_me.json"
        };
        mapping::profile(&self.read(name)?)
    }

    async fn list_chats(&self, limit: usize) -> Result<Vec<Chat>> {
        self.ready()?;
        self.chat_ids()?
            .into_iter()
            .take(limit)
            .map(|id| self.load_chat(id))
            .collect()
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        self.ready()?;
        let id = chat_id.parse::<i64>().map_err(|_| Error::NotFound {
            provider: PROVIDER,
            resource: format!("chat `{chat_id}`"),
        })?;
        self.load_chat(id)
    }

    async fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        self.ready()?;
        let id = chat_id.parse::<i64>().map_err(|_| Error::NotFound {
            provider: PROVIDER,
            resource: format!("chat `{chat_id}`"),
        })?;
        let response = self.read(&format!("history_{id}.json"))?;
        let messages = response
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Protocol {
                provider: PROVIDER,
                message: "fixture has no `messages` array".to_owned(),
            })?;
        messages
            .iter()
            .take(limit)
            .map(|message| mapping::message(message, id))
            .collect()
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>> {
        let all = match chat_id {
            Some(chat_id) => self.list_messages(chat_id, usize::MAX).await?,
            None => {
                let mut all = Vec::new();
                for id in self.chat_ids()? {
                    if let Ok(messages) = self.list_messages(&id.to_string(), usize::MAX).await {
                        all.extend(messages);
                    }
                }
                all
            }
        };
        let needle = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|message| message.text.to_lowercase().contains(&needle))
            .take(limit)
            .collect())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        _text: &str,
        _reply_to: Option<&str>,
    ) -> Result<String> {
        self.ready()?;
        if chat_id.is_empty() {
            return Err(Error::InvalidInput {
                message: "send needs a chat id".to_owned(),
            });
        }
        Ok("9001".to_owned())
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        self.ready()?;
        let response = self.read(&format!("members_{chat_id}.json"))?;
        let members = response
            .get("members")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Protocol {
                provider: PROVIDER,
                message: "fixture has no `members` array".to_owned(),
            })?;
        let self_id = self
            .read("get_me.json")
            .ok()
            .and_then(|user| user.get("id").and_then(Value::as_i64));
        members
            .iter()
            .filter_map(|entry| entry.get("user"))
            .map(|user| mapping::member(user, self_id))
            .collect()
    }

    async fn mark_read(&self, _chat_id: &str) -> Result<()> {
        self.ready()
    }

    async fn authorization(&self) -> Result<(String, bool, Option<String>)> {
        let summary = mapping::authorization_state(&self.read("auth_state_ready.json")?);
        let authenticated = summary.authenticated && !self.closed();
        Ok((summary.state, authenticated, summary.next_action))
    }

    async fn login(&self, _params: &Value) -> Result<(String, bool, Option<String>)> {
        self.authorization().await
    }
}
