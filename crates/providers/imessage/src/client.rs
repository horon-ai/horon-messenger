use std::time::Duration;

use serde_json::{Value, json};

use horon_messenger_core::{Error, Result};
use horon_messenger_provider_common::{SidecarSession, SidecarSpec};

use crate::PROVIDER;
use crate::models::{ChatsList, ImsgChat, ImsgMessage, MessagesHistory, SendResponse};

const FULL_DISK_ACCESS_HINTS: [&str; 3] = [
    "full disk access",
    "cannot access messages database",
    "unable to open database",
];
const AUTOMATION_HINTS: [&str; 2] = ["not authorized to send", "apple events"];

/// A reusable handle to the `imsg rpc` child, spoken to over line-delimited JSON-RPC.
#[derive(Debug, Clone)]
pub(crate) struct IMessageClient {
    session: SidecarSession,
}

impl IMessageClient {
    pub(crate) async fn connect(spec: SidecarSpec, timeout: Duration) -> Result<Self> {
        Ok(Self {
            session: SidecarSession::new(spec, timeout),
        })
    }

    pub(crate) async fn list_chats(&mut self, limit: usize) -> Result<Vec<ImsgChat>> {
        let value = self
            .call("chats.list", json!({ "limit": limit }), true)
            .await?;
        let parsed: ChatsList = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.chats)
    }

    pub(crate) async fn history(
        &mut self,
        chat_id: u64,
        limit: usize,
        start: Option<&str>,
    ) -> Result<Vec<ImsgMessage>> {
        let mut params = json!({ "chat_id": chat_id, "limit": limit });
        if let Some(start) = start {
            params["start"] = Value::String(start.to_owned());
        }
        let value = self.call("messages.history", params, true).await?;
        let parsed: MessagesHistory = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.messages)
    }

    pub(crate) async fn send(&mut self, params: Value) -> Result<SendResponse> {
        let value = self.call("send", params, false).await?;
        serde_json::from_value(value).map_err(payload_error)
    }

    async fn call(&self, method: &str, params: Value, idempotent: bool) -> Result<Value> {
        self.session
            .call(method, params, idempotent)
            .await
            .map_err(classify)
    }
}

fn payload_error(error: serde_json::Error) -> Error {
    Error::Protocol {
        provider: PROVIDER,
        message: format!("imsg returned an unexpected payload: {error}"),
    }
}

/// Rewrites an `imsg` permission failure into an error that names the missing macOS permission.
fn classify(error: Error) -> Error {
    let text = error.to_string().to_lowercase();
    if FULL_DISK_ACCESS_HINTS
        .iter()
        .any(|hint| text.contains(hint))
    {
        return Error::AuthFailed {
            provider: PROVIDER,
            message: "imsg cannot read the Messages database; grant Full Disk Access to the app that launched horon-messenger (System Settings -> Privacy & Security -> Full Disk Access)".to_owned(),
        };
    }
    if AUTOMATION_HINTS.iter().any(|hint| text.contains(hint)) {
        return Error::AuthFailed {
            provider: PROVIDER,
            message: "imsg is not allowed to control Messages.app; grant Automation access to Messages in System Settings -> Privacy & Security".to_owned(),
        };
    }
    error
}
