use std::sync::Arc;

use futures_util::StreamExt;
use horon_messenger_sdk::{ChatQuery, Client, Envelope, MessageQuery, Meta, ProviderKind, SendRequest};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone)]
struct MessengerServer {
    client: Arc<Client>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ProviderInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListChatsInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Case-insensitive substring filter on chat names.
    #[serde(default)]
    search: Option<String>,
    /// Maximum number of chats to return.
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListMessagesInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Chat identifier, as returned by list_chats.
    chat_id: String,
    /// Case-insensitive substring filter on message text.
    #[serde(default)]
    search: Option<String>,
    /// Maximum number of messages to return.
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SendMessageInput {
    /// Provider to send through: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Chat identifier, as returned by list_chats.
    chat_id: String,
    /// Message text.
    text: String,
    /// Message id to quote, when the provider supports replies.
    #[serde(default)]
    reply_to: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MembersInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Chat identifier, as returned by list_chats.
    chat_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ChatInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Chat identifier, as returned by list_chats.
    chat_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchMessagesInput {
    /// Provider to query: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Restrict the search to one chat; omit to search every chat the provider allows.
    #[serde(default)]
    chat_id: Option<String>,
    /// Case-insensitive substring filter on message text.
    #[serde(default)]
    search: Option<String>,
    /// Maximum number of messages to return.
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListenInput {
    /// Provider to listen on: kakaotalk, discord, telegram, channeltalk or mock.
    provider: ProviderKind,
    /// Restrict the stream to one chat; omit to listen on every chat.
    #[serde(default)]
    chat_id: Option<String>,
    /// Stop after this many events (default 20).
    #[serde(default)]
    limit: Option<usize>,
}

fn tool_result(envelope: Envelope) -> CallToolResult {
    let ok = envelope.ok;
    let value = envelope.into_value();
    if ok {
        CallToolResult::structured(value)
    } else {
        CallToolResult::structured_error(value)
    }
}

#[tool_router(router = tool_router)]
impl MessengerServer {
    #[tool(
        name = "list_providers",
        description = "List the messenger providers this server can reach, with each one's supported operations."
    )]
    async fn list_providers(&self) -> CallToolResult {
        tool_result(Envelope::success(self.client.describe(), Meta::now(None)))
    }

    #[tool(name = "whoami", description = "Show the authenticated account a provider is using.")]
    async fn whoami(&self, Parameters(input): Parameters<ProviderInput>) -> CallToolResult {
        let envelope = Envelope::from_result(
            self.client.whoami(input.provider).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "list_chats",
        description = "List chats for a provider, optionally filtered by a name substring."
    )]
    async fn list_chats(&self, Parameters(input): Parameters<ListChatsInput>) -> CallToolResult {
        let query = ChatQuery {
            search: input.search,
            limit: input.limit,
        };
        let envelope = Envelope::from_result(
            self.client.list_chats(input.provider, &query).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "list_messages",
        description = "List messages in one chat, optionally filtered by a text substring."
    )]
    async fn list_messages(&self, Parameters(input): Parameters<ListMessagesInput>) -> CallToolResult {
        let query = MessageQuery {
            search: input.search,
            limit: input.limit,
            before: None,
            after: None,
        };
        let envelope = Envelope::from_result(
            self.client.list_messages(input.provider, &input.chat_id, &query).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "send_message",
        description = "Send one message to a chat through a provider."
    )]
    async fn send_message(&self, Parameters(input): Parameters<SendMessageInput>) -> CallToolResult {
        let request = SendRequest {
            chat_id: input.chat_id,
            text: input.text,
            reply_to: input.reply_to,
            attachments: Vec::new(),
            idempotency_key: None,
        };
        let envelope = Envelope::from_result(
            self.client.send_message(input.provider, &request).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(name = "list_members", description = "List the members of one chat.")]
    async fn list_members(&self, Parameters(input): Parameters<MembersInput>) -> CallToolResult {
        let envelope = Envelope::from_result(
            self.client.list_members(input.provider, &input.chat_id).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(name = "get_chat", description = "Show one chat by id.")]
    async fn get_chat(&self, Parameters(input): Parameters<ChatInput>) -> CallToolResult {
        let envelope = Envelope::from_result(
            self.client.get_chat(input.provider, &input.chat_id).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "search_messages",
        description = "Search messages by a text substring, in one chat or across every chat."
    )]
    async fn search_messages(&self, Parameters(input): Parameters<SearchMessagesInput>) -> CallToolResult {
        let query = MessageQuery {
            search: input.search,
            limit: input.limit,
            before: None,
            after: None,
        };
        let envelope = Envelope::from_result(
            self.client
                .search_messages(input.provider, input.chat_id.as_deref(), &query)
                .await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(name = "mark_read", description = "Mark a chat as read.")]
    async fn mark_read(&self, Parameters(input): Parameters<ChatInput>) -> CallToolResult {
        let envelope = Envelope::from_result(
            self.client.mark_read(input.provider, &input.chat_id).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "snapshot",
        description = "Print a provider snapshot: the profile and every chat."
    )]
    async fn snapshot(&self, Parameters(input): Parameters<ProviderInput>) -> CallToolResult {
        let envelope = Envelope::from_result(
            self.client.snapshot(input.provider).await,
            Meta::now(Some(input.provider)),
        );
        tool_result(envelope)
    }

    #[tool(
        name = "listen",
        description = "Collect recent events (message, read, typing) from a provider, up to a limit."
    )]
    async fn listen(&self, Parameters(input): Parameters<ListenInput>) -> CallToolResult {
        let limit = input.limit.unwrap_or(20);
        let mut stream = match self.client.listen(input.provider, input.chat_id.as_deref()).await {
            Ok(stream) => stream,
            Err(error) => {
                return tool_result(Envelope::failure(&error, Meta::now(Some(input.provider))));
            }
        };
        let mut events = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    events.push(event);
                    if events.len() >= limit {
                        break;
                    }
                }
                Err(error) => {
                    return tool_result(Envelope::failure(&error, Meta::now(Some(input.provider))));
                }
            }
        }
        tool_result(Envelope::success(events, Meta::now(Some(input.provider))))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MessengerServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "horon-messenger",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
            "Multi-messenger MCP server: read and send across KakaoTalk, Discord, Telegram and Channel Talk through one tool set.".to_owned(),
        )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let client = Arc::new(Client::builder().build()?);
    let server = MessengerServer {
        client,
        tool_router: MessengerServer::tool_router(),
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
