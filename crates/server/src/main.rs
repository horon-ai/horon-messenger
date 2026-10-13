use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use clap::Parser;
use horon_messenger_sdk::{
    ChatQuery, Client, Envelope, MessageQuery, Meta, ProviderKind, SendRequest,
};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

#[derive(Debug, Parser)]
#[command(
    name = "horon-messenger-server",
    version,
    about = "HTTP API for horon-messenger"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8787)]
    port: u16,

    #[arg(long, env = "HORON_MESSENGER_CONFIG_DIR")]
    config_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
}

#[derive(Debug, Deserialize)]
struct ChatsParams {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MessagesParams {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SendBody {
    text: String,
    #[serde(default)]
    reply_to: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let mut builder = Client::builder();
    if let Some(dir) = args.config_dir {
        builder = builder.config_dir(dir);
    }
    let client = Arc::new(builder.build()?);
    let app = router(AppState { client });

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port)).await?;
    tracing::info!(
        "horon-messenger-server listening on http://{}:{}",
        args.host,
        args.port
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/providers", get(list_providers))
        .route("/v1/providers/{kind}/whoami", get(whoami))
        .route("/v1/providers/{kind}/chats", get(list_chats))
        .route(
            "/v1/providers/{kind}/chats/{chat_id}/messages",
            get(list_messages).post(send_message),
        )
        .route(
            "/v1/providers/{kind}/chats/{chat_id}/members",
            get(list_members),
        )
        .route("/v1/providers/{kind}/snapshot", get(snapshot))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<Envelope> {
    Json(Envelope::success(
        serde_json::json!({ "status": "ok" }),
        Meta::now(None),
    ))
}

async fn list_providers(State(state): State<AppState>) -> Json<Envelope> {
    Json(Envelope::success(state.client.describe(), Meta::now(None)))
}

async fn whoami(State(state): State<AppState>, Path(kind): Path<ProviderKind>) -> Json<Envelope> {
    Json(Envelope::from_result(
        state.client.whoami(kind).await,
        Meta::now(Some(kind)),
    ))
}

async fn list_chats(
    State(state): State<AppState>,
    Path(kind): Path<ProviderKind>,
    Query(params): Query<ChatsParams>,
) -> Json<Envelope> {
    let query = ChatQuery {
        search: params.search,
        limit: params.limit,
    };
    Json(Envelope::from_result(
        state.client.list_chats(kind, &query).await,
        Meta::now(Some(kind)),
    ))
}

async fn list_messages(
    State(state): State<AppState>,
    Path((kind, chat_id)): Path<(ProviderKind, String)>,
    Query(params): Query<MessagesParams>,
) -> Json<Envelope> {
    let query = MessageQuery {
        search: params.search,
        limit: params.limit,
        before: None,
        after: None,
    };
    Json(Envelope::from_result(
        state.client.list_messages(kind, &chat_id, &query).await,
        Meta::now(Some(kind)),
    ))
}

async fn send_message(
    State(state): State<AppState>,
    Path((kind, chat_id)): Path<(ProviderKind, String)>,
    Json(body): Json<SendBody>,
) -> Json<Envelope> {
    let request = SendRequest {
        chat_id,
        text: body.text,
        reply_to: body.reply_to,
        attachments: Vec::new(),
        idempotency_key: None,
    };
    Json(Envelope::from_result(
        state.client.send_message(kind, &request).await,
        Meta::now(Some(kind)),
    ))
}

async fn list_members(
    State(state): State<AppState>,
    Path((kind, chat_id)): Path<(ProviderKind, String)>,
) -> Json<Envelope> {
    Json(Envelope::from_result(
        state.client.list_members(kind, &chat_id).await,
        Meta::now(Some(kind)),
    ))
}

async fn snapshot(State(state): State<AppState>, Path(kind): Path<ProviderKind>) -> Json<Envelope> {
    Json(Envelope::from_result(
        state.client.snapshot(kind).await,
        Meta::now(Some(kind)),
    ))
}
