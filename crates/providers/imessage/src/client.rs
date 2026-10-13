use std::time::Duration;

use serde_json::{Value, json};

use hyper_messenger_core::{Error, Result};
use hyper_messenger_provider_common::{JsonRpcProcess, SidecarSpec};

use crate::PROVIDER;
use crate::models::{ChatsList, ImsgChat, ImsgMessage, MessagesHistory, SendResponse};

const FULL_DISK_ACCESS_HINTS: [&str; 3] = [
    "full disk access",
    "cannot access messages database",
    "unable to open database",
];
const AUTOMATION_HINTS: [&str; 2] = ["not authorized to send", "apple events"];

/// A live `imsg rpc` child, spoken to over line-delimited JSON-RPC.
pub(crate) struct IMessageClient {
    rpc: JsonRpcProcess,
}

impl IMessageClient {
    pub(crate) async fn connect(spec: SidecarSpec, timeout: Duration) -> Result<Self> {
        let rpc = JsonRpcProcess::start_with_timeout(spec, timeout).await?;
        Ok(Self { rpc })
    }

    pub(crate) async fn list_chats(&mut self, limit: usize) -> Result<Vec<ImsgChat>> {
        let value = self.call("chats.list", json!({ "limit": limit })).await?;
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
        let value = self.call("messages.history", params).await?;
        let parsed: MessagesHistory = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.messages)
    }

    pub(crate) async fn send(&mut self, params: Value) -> Result<SendResponse> {
        let value = self.call("send", params).await?;
        serde_json::from_value(value).map_err(payload_error)
    }

    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.rpc.shutdown().await
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.rpc.call(method, params).await.map_err(classify)
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
            message: "imsg cannot read the Messages database; grant Full Disk Access to the app that launched hyper-messenger (System Settings -> Privacy & Security -> Full Disk Access)".to_owned(),
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
