use std::sync::Arc;

use async_trait::async_trait;

use crate::auth::AuthProvider;
use crate::capability::Capabilities;
use crate::error::{Error, Result};
use crate::kind::ProviderKind;
use crate::model::{
    Chat, ChatQuery, EventStream, Member, MessagePage, MessageQuery, Profile, SendRequest, SendResult, Snapshot,
};

/// One messenger platform. Every surface reaches a platform only through this
/// trait, so the CLI, MCP server, API server and UI share one behaviour set.
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind;

    fn capabilities(&self) -> Capabilities;

    /// The authentication contract, when this provider exposes one.
    fn auth(&self) -> Option<&dyn AuthProvider> {
        None
    }

    /// The error for an operation this provider leaves to the default: an
    /// advertised capability reports `NotImplemented`, any other `Unsupported`.
    fn gate(&self, capability: Capabilities) -> Error {
        let operation = capability.name();
        if self.capabilities().contains(capability) {
            Error::NotImplemented {
                provider: self.kind(),
                operation,
            }
        } else {
            Error::Unsupported {
                provider: self.kind(),
                operation,
            }
        }
    }

    async fn whoami(&self) -> Result<Profile> {
        Err(self.gate(Capabilities::WHOAMI))
    }

    async fn list_chats(&self, _query: &ChatQuery) -> Result<Vec<Chat>> {
        Err(self.gate(Capabilities::LIST_CHATS))
    }

    async fn get_chat(&self, _chat_id: &str) -> Result<Chat> {
        Err(self.gate(Capabilities::GET_CHAT))
    }

    async fn list_messages(&self, _chat_id: &str, _query: &MessageQuery) -> Result<MessagePage> {
        Err(self.gate(Capabilities::LIST_MESSAGES))
    }

    async fn search_messages(&self, _chat_id: Option<&str>, _query: &MessageQuery) -> Result<MessagePage> {
        Err(self.gate(Capabilities::SEARCH_MESSAGES))
    }

    async fn send_message(&self, _request: &SendRequest) -> Result<SendResult> {
        Err(self.gate(Capabilities::SEND_MESSAGE))
    }

    async fn list_members(&self, _chat_id: &str) -> Result<Vec<Member>> {
        Err(self.gate(Capabilities::LIST_MEMBERS))
    }

    async fn mark_read(&self, _chat_id: &str) -> Result<()> {
        Err(self.gate(Capabilities::MARK_READ))
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        Err(self.gate(Capabilities::SNAPSHOT))
    }

    /// A stream of events from a provider, or one chat when `chat_id` is set.
    async fn listen(&self, _chat_id: Option<&str>) -> Result<EventStream> {
        Err(self.gate(Capabilities::LISTEN))
    }
}

pub type DynProvider = Arc<dyn Provider>;
