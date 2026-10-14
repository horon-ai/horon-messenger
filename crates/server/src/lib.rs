use std::sync::Arc;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use horon_messenger_sdk::{
    ChatQuery, Client, Envelope, MessageQuery, Meta, ProviderKind, SendRequest,
};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

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

#[derive(Debug, Deserialize)]
struct SearchParams {
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ListenParams {
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// Builds the HTTP API router around a single shared SDK client.
pub fn build_router(client: Arc<Client>) -> Router {
    let state = AppState { client };
    Router::new()
        .route("/health", get(health))
        .route("/v1/providers", get(list_providers))
        .route("/v1/providers/{kind}/whoami", get(whoami))
        .route("/v1/providers/{kind}/chats", get(list_chats))
        .route(
            "/v1/providers/{kind}/chats/{chat_id}",
            get(get_chat),
        )
        .route(
            "/v1/providers/{kind}/chats/{chat_id}/messages",
            get(list_messages).post(send_message),
        )
        .route(
            "/v1/providers/{kind}/chats/{chat_id}/members",
            get(list_members),
        )
        .route(
            "/v1/providers/{kind}/chats/{chat_id}/read",
            post(mark_read),
        )
        .route("/v1/providers/{kind}/messages", get(search_messages))
        .route("/v1/providers/{kind}/events", get(listen))
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

async fn get_chat(
    State(state): State<AppState>,
    Path((kind, chat_id)): Path<(ProviderKind, String)>,
) -> Json<Envelope> {
    Json(Envelope::from_result(
        state.client.get_chat(kind, &chat_id).await,
        Meta::now(Some(kind)),
    ))
}

async fn search_messages(
    State(state): State<AppState>,
    Path(kind): Path<ProviderKind>,
    Query(params): Query<SearchParams>,
) -> Json<Envelope> {
    let query = MessageQuery {
        search: params.search,
        limit: params.limit,
        before: None,
        after: None,
    };
    Json(Envelope::from_result(
        state
            .client
            .search_messages(kind, params.chat_id.as_deref(), &query)
            .await,
        Meta::now(Some(kind)),
    ))
}

async fn mark_read(
    State(state): State<AppState>,
    Path((kind, chat_id)): Path<(ProviderKind, String)>,
) -> Json<Envelope> {
    Json(Envelope::from_result(
        state.client.mark_read(kind, &chat_id).await,
        Meta::now(Some(kind)),
    ))
}

async fn listen(
    State(state): State<AppState>,
    Path(kind): Path<ProviderKind>,
    Query(params): Query<ListenParams>,
) -> Response {
    let stream = match state.client.listen(kind, params.chat_id.as_deref()).await {
        Ok(stream) => stream,
        Err(error) => {
            return Json(Envelope::failure(&error, Meta::now(Some(kind)))).into_response();
        }
    };
    let stream = stream.map(|item| {
        let line = match item {
            Ok(event) => serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_owned()),
            Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        Ok::<Bytes, std::io::Error>(Bytes::from(format!("{line}\n")))
    });
    let stream: futures_util::stream::BoxStream<'static, Result<Bytes, std::io::Error>> =
        match params.limit {
            Some(limit) => stream.take(limit).boxed(),
            None => stream.boxed(),
        };
    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(stream))
        .expect("build the ndjson response")
}
