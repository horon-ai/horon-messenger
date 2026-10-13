//! The transport a [`crate::TelegramProvider`] drives: the real TDLib client or the `qa-fake` replay.

use async_trait::async_trait;
use horon_messenger_core::{Chat, Member, Message, Profile, Result};
use serde_json::Value;

/// The operations a Telegram transport answers, independent of how it is backed.
#[async_trait]
pub trait TelegramBackend: Send + Sync {
    /// The authenticated account.
    async fn whoami(&self) -> Result<Profile>;

    /// Lists chats, up to `limit`.
    async fn list_chats(&self, limit: usize) -> Result<Vec<Chat>>;

    /// Fetches one chat by its numeric id.
    async fn get_chat(&self, chat_id: &str) -> Result<Chat>;

    /// Lists recent messages in a chat, newest first, up to `limit`.
    async fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>>;

    /// Searches messages across one chat or every chat.
    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>>;

    /// Sends a text message and returns its id.
    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<String>;

    /// Lists the members of a chat.
    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>>;

    /// Marks every message in a chat as read.
    async fn mark_read(&self, chat_id: &str) -> Result<()>;

    /// The current authorization state as `(state, authenticated, next_action)`.
    async fn authorization(&self) -> Result<(String, bool, Option<String>)>;

    /// Submits the login inputs carried in `params` and returns the new state.
    async fn login(&self, params: &Value) -> Result<(String, bool, Option<String>)>;
}
