use std::time::Duration;

use serde_json::{Value, json};

use horon_messenger_core::{Error, Result};
use horon_messenger_provider_common::{JsonRpcProcess, SidecarSpec};

use crate::PROVIDER;
use crate::models::{
    ChatsList, LineChat, LineMember, LineMessage, LineProfile, LineSendResult, LoginResponse,
    MembersList, MessagesList,
};

const AUTH_HINTS: [&str; 6] = [
    "auth_failed",
    "invalid token",
    "invalid auth",
    "authentication failed",
    "login failed",
    "unauthorized",
];
const NOT_FOUND_HINTS: [&str; 2] = ["not_found", "not found"];

/// A live LINE sidecar child, spoken to over line-delimited JSON-RPC.
pub(crate) struct LineClient {
    rpc: JsonRpcProcess,
}

impl LineClient {
    pub(crate) async fn connect(spec: SidecarSpec, timeout: Duration) -> Result<Self> {
        Ok(Self {
            rpc: JsonRpcProcess::start_with_timeout(spec, timeout).await?,
        })
    }

    pub(crate) async fn login(&mut self, params: Value) -> Result<LoginResponse> {
        let value = self.call("auth.login", params).await?;
        serde_json::from_value(value).map_err(payload_error)
    }

    pub(crate) async fn whoami(&mut self) -> Result<LineProfile> {
        let value = self.call("whoami", json!({})).await?;
        serde_json::from_value(value).map_err(payload_error)
    }

    pub(crate) async fn list_chats(&mut self, limit: usize) -> Result<Vec<LineChat>> {
        let value = self.call("chats.list", json!({ "limit": limit })).await?;
        let parsed: ChatsList = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.chats)
    }

    pub(crate) async fn list_messages(
        &mut self,
        chat_id: &str,
        count: usize,
    ) -> Result<Vec<LineMessage>> {
        let value = self
            .call(
                "messages.list",
                json!({ "chat_id": chat_id, "count": count }),
            )
            .await?;
        let parsed: MessagesList = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.messages)
    }

    pub(crate) async fn list_members(&mut self, chat_id: &str) -> Result<Vec<LineMember>> {
        let value = self
            .call("members.list", json!({ "chat_id": chat_id }))
            .await?;
        let parsed: MembersList = serde_json::from_value(value).map_err(payload_error)?;
        Ok(parsed.members)
    }

    pub(crate) async fn send(&mut self, chat_id: &str, text: &str) -> Result<LineSendResult> {
        let value = self
            .call("message.send", json!({ "chat_id": chat_id, "text": text }))
            .await?;
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
        message: format!("line sidecar returned an unexpected payload: {error}"),
    }
}

fn classify(error: Error) -> Error {
    let Error::Protocol { message, .. } = &error else {
        return error;
    };
    let text = message.to_lowercase();
    if AUTH_HINTS.iter().any(|hint| text.contains(hint)) {
        return Error::AuthFailed {
            provider: PROVIDER,
            message: message.clone(),
        };
    }
    if NOT_FOUND_HINTS.iter().any(|hint| text.contains(hint)) {
        return Error::NotFound {
            provider: PROVIDER,
            resource: message.clone(),
        };
    }
    error
}
