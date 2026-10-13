//! The real TDLib backend, driven through the `tdjson` JSON API.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use horon_messenger_core::{
    Chat, ConfigPaths, CredentialStore, Error, FileCredentialStore, Member, Message, Profile,
    ProviderKind, Result,
};
use serde_json::{Value, json};

use tdlib_rs as _;

use crate::mapping;
use crate::tdlib_library::TdlibLibrary;
use crate::transport::TelegramBackend;

const PROVIDER: ProviderKind = ProviderKind::Telegram;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// The API credentials and phone the TDLib client needs, read from stored config.
#[derive(Debug, Clone, Default)]
pub struct TdlibConfig {
    pub api_id: i64,
    pub api_hash: String,
    pub phone_number: Option<String>,
}

impl TdlibConfig {
    /// Reads the credentials for `account_id` from the provider credential store.
    pub fn load(paths: &ConfigPaths, account_id: &str) -> Result<Self> {
        let store = FileCredentialStore::new(paths.clone());
        let value = store
            .load(PROVIDER, account_id)?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        let api_id =
            value
                .get("api_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::InvalidInput {
                    message: "Telegram credentials need a numeric `api_id`".to_owned(),
                })?;
        let api_hash = value
            .get("api_hash")
            .and_then(Value::as_str)
            .filter(|hash| !hash.is_empty())
            .ok_or_else(|| Error::InvalidInput {
                message: "Telegram credentials need a non-empty `api_hash`".to_owned(),
            })?
            .to_owned();
        Ok(Self {
            api_id,
            api_hash,
            phone_number: value
                .get("phone_number")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}

struct Loaded {
    library: Arc<TdlibLibrary>,
    client_id: i32,
    state: Value,
}

/// The real TDLib backend: one `tdjson` client, its authorization state, and the JSON API.
pub struct TdlibBackend {
    config: TdlibConfig,
    account_id: String,
    paths: ConfigPaths,
    loaded: Mutex<Option<Loaded>>,
}

impl TdlibBackend {
    /// Builds a backend that loads `config` and stores TDLib state under `paths`.
    pub fn new(config: TdlibConfig, account_id: String, paths: ConfigPaths) -> Self {
        Self {
            config,
            account_id,
            paths,
            loaded: Mutex::new(None),
        }
    }

    /// Opens the TDLib client, loading the shared library on first use.
    pub fn connect(&self) -> Result<()> {
        let mut guard = self.lock()?;
        if guard.is_some() {
            return Ok(());
        }
        let library = Arc::new(TdlibLibrary::load()?);
        let client_id = library.create_client();
        let state = self.drive(&library, client_id, CONNECT_TIMEOUT)?;
        *guard = Some(Loaded {
            library,
            client_id,
            state,
        });
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<Loaded>>> {
        self.loaded.lock().map_err(|_| Error::Internal {
            message: "TDLib backend state is poisoned".to_owned(),
        })
    }

    fn handle(&self) -> Result<(Arc<TdlibLibrary>, i32, Value)> {
        let guard = self.lock()?;
        guard
            .as_ref()
            .map(|loaded| {
                (
                    loaded.library.clone(),
                    loaded.client_id,
                    loaded.state.clone(),
                )
            })
            .ok_or(Error::Internal {
                message: "TDLib client is not open".to_owned(),
            })
    }

    fn ready_handle(&self) -> Result<(Arc<TdlibLibrary>, i32)> {
        self.connect()?;
        let (library, client_id, state) = self.handle()?;
        if state.get("@type").and_then(Value::as_str) != Some("authorizationStateReady") {
            return Err(Error::AuthRequired { provider: PROVIDER });
        }
        Ok((library, client_id))
    }

    fn drive(&self, library: &TdlibLibrary, client_id: i32, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let state = call(
                library,
                client_id,
                json!({ "@type": "getAuthorizationState" }),
                deadline,
            )?;
            match state.get("@type").and_then(Value::as_str) {
                Some("authorizationStateWaitTdlibParameters") => {
                    self.set_parameters(library, client_id, deadline)?;
                }
                Some("authorizationStateClosed") => {
                    return Err(Error::Protocol {
                        provider: PROVIDER,
                        message: "the TDLib client was closed".to_owned(),
                    });
                }
                _ => return Ok(state),
            }
        }
    }

    fn set_parameters(
        &self,
        library: &TdlibLibrary,
        client_id: i32,
        deadline: Instant,
    ) -> Result<()> {
        let provider_dir = self.paths.ensure_provider_dir(PROVIDER)?;
        let database = provider_dir.join(format!("tdlib-{}", self.account_id));
        let files = provider_dir.join(format!("files-{}", self.account_id));
        call(
            library,
            client_id,
            json!({
                "@type": "setTdlibParameters",
                "use_test_dc": false,
                "api_id": self.config.api_id,
                "api_hash": self.config.api_hash,
                "system_language_code": "en",
                "device_model": "horon-messenger",
                "application_version": env!("CARGO_PKG_VERSION"),
                "database_directory": database,
                "files_directory": files,
                "use_file_database": true,
                "use_chat_info_database": true,
                "use_message_database": true,
            }),
            deadline,
        )?;
        Ok(())
    }

    fn run<T>(&self, f: impl FnOnce(&TdlibLibrary, i32, Instant) -> Result<T>) -> Result<T> {
        let (library, client_id) = self.ready_handle()?;
        f(&library, client_id, Instant::now() + DEFAULT_TIMEOUT)
    }
}

#[async_trait]
impl TelegramBackend for TdlibBackend {
    async fn whoami(&self) -> Result<Profile> {
        self.run(|library, client_id, deadline| {
            let user = call(library, client_id, json!({ "@type": "getMe" }), deadline)?;
            mapping::profile(&user)
        })
    }

    async fn list_chats(&self, limit: usize) -> Result<Vec<Chat>> {
        self.run(|library, client_id, deadline| {
            fetch_chats(library, client_id, deadline, limit)?
                .iter()
                .map(mapping::chat)
                .collect()
        })
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        self.run(|library, client_id, deadline| {
            let id = resolve_chat_id(library, client_id, chat_id, deadline)?;
            let chat = call(
                library,
                client_id,
                json!({ "@type": "getChat", "chat_id": id }),
                deadline,
            )?;
            mapping::chat(&chat)
        })
    }

    async fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        self.run(|library, client_id, deadline| {
            let id = resolve_chat_id(library, client_id, chat_id, deadline)?;
            let response = call(
                library,
                client_id,
                json!({
                    "@type": "getChatHistory",
                    "chat_id": id,
                    "from_message_id": 0,
                    "offset": 0,
                    "limit": limit,
                    "only_local": false,
                }),
                deadline,
            )?;
            messages_from(&response, id, limit)
        })
    }

    async fn search_messages(
        &self,
        chat_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>> {
        self.run(|library, client_id, deadline| {
            let (request, id) = match chat_id {
                Some(reference) => {
                    let id = resolve_chat_id(library, client_id, reference, deadline)?;
                    (
                        json!({
                            "@type": "searchChatMessages",
                            "chat_id": id,
                            "query": query,
                            "from_message_id": 0,
                            "offset": 0,
                            "limit": limit,
                        }),
                        id,
                    )
                }
                None => (
                    json!({
                        "@type": "searchMessages",
                        "chat_list": { "@type": "chatListMain" },
                        "query": query,
                        "offset": 0,
                        "limit": limit,
                    }),
                    0,
                ),
            };
            messages_from(&call(library, client_id, request, deadline)?, id, limit)
        })
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<String> {
        self.run(|library, client_id, deadline| {
            let id = resolve_chat_id(library, client_id, chat_id, deadline)?;
            let reply = reply_to.and_then(|reply| reply.parse::<i64>().ok());
            let message = call(
                library,
                client_id,
                json!({
                    "@type": "sendMessage",
                    "chat_id": id,
                    "input_message_content": {
                        "@type": "inputMessageText",
                        "text": { "@type": "formattedText", "text": text, "entities": [] },
                    },
                    "reply_to": reply.map(|message_id| json!({
                        "@type": "inputMessageReplyToMessage",
                        "message_id": message_id,
                    })),
                }),
                deadline,
            )?;
            message
                .get("id")
                .and_then(Value::as_i64)
                .map(|id| id.to_string())
                .ok_or_else(|| Error::Protocol {
                    provider: PROVIDER,
                    message: "TDLib sendMessage returned no message id".to_owned(),
                })
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        self.run(|library, client_id, deadline| {
            let id = resolve_chat_id(library, client_id, chat_id, deadline)?;
            let self_id = call(library, client_id, json!({ "@type": "getMe" }), deadline)
                .ok()
                .and_then(|me| me.get("id").and_then(Value::as_i64));
            let response = call(
                library,
                client_id,
                json!({ "@type": "getChatMembers", "chat_id": id, "limit": 200 }),
                deadline,
            )?;
            let members = response
                .get("members")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            members
                .iter()
                .filter_map(|entry| entry.get("user"))
                .map(|user| mapping::member(user, self_id))
                .collect()
        })
    }

    async fn mark_read(&self, chat_id: &str) -> Result<()> {
        self.run(|library, client_id, deadline| {
            let id = resolve_chat_id(library, client_id, chat_id, deadline)?;
            call(
                library,
                client_id,
                json!({
                    "@type": "viewMessages",
                    "chat_id": id,
                    "message_ids": [],
                    "force_read": true,
                }),
                deadline,
            )?;
            Ok(())
        })
    }

    async fn authorization(&self) -> Result<(String, bool, Option<String>)> {
        self.connect()?;
        let (_, _, state) = self.handle()?;
        let summary = mapping::authorization_state(&state);
        Ok((summary.state, summary.authenticated, summary.next_action))
    }

    async fn login(&self, params: &Value) -> Result<(String, bool, Option<String>)> {
        self.connect()?;
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        let (library, client_id, state) = self.handle()?;
        if let Some(request) = login_request(&state, params) {
            let response = call(&library, client_id, request, deadline)?;
            if let Some(loaded) = self.lock()?.as_mut() {
                loaded.state = response;
            }
        }
        self.authorization().await
    }
}

fn call(library: &TdlibLibrary, client_id: i32, query: Value, deadline: Instant) -> Result<Value> {
    let extra = format!(
        "hm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let mut request = query;
    request["@extra"] = Value::String(extra.clone());
    library.send(client_id, &request.to_string())?;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Network {
                provider: PROVIDER,
                message: format!(
                    "TDLib did not answer `{}` before the timeout",
                    request
                        .get("@type")
                        .and_then(Value::as_str)
                        .unwrap_or("request")
                ),
            });
        }
        let Some(raw) = receive(library)? else {
            continue;
        };
        if raw.get("@extra").and_then(Value::as_str) == Some(extra.as_str()) {
            return unwrap_response(&raw);
        }
    }
}

fn receive(library: &TdlibLibrary) -> Result<Option<Value>> {
    let Some(text) = library.receive(0.25) else {
        return Ok(None);
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(mapping::parse_response(&text)?))
}

fn fetch_chats(
    library: &TdlibLibrary,
    client_id: i32,
    deadline: Instant,
    limit: usize,
) -> Result<Vec<Value>> {
    let response = call(
        library,
        client_id,
        json!({
            "@type": "getChats",
            "chat_list": { "@type": "chatListMain" },
            "limit": limit,
        }),
        deadline,
    )?;
    let mut chats = Vec::new();
    for id in mapping::chat_ids(&response)?.into_iter().take(limit) {
        chats.push(call(
            library,
            client_id,
            json!({ "@type": "getChat", "chat_id": id }),
            deadline,
        )?);
    }
    Ok(chats)
}

fn resolve_chat_id(
    library: &TdlibLibrary,
    client_id: i32,
    reference: &str,
    deadline: Instant,
) -> Result<i64> {
    if let Ok(id) = reference.parse::<i64>() {
        return Ok(id);
    }
    let chat = call(
        library,
        client_id,
        json!({ "@type": "searchPublicChat", "username": reference.trim_start_matches('@') }),
        deadline,
    )?;
    chat.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::NotFound {
            provider: PROVIDER,
            resource: format!("chat `{reference}`"),
        })
}

fn unwrap_response(raw: &Value) -> Result<Value> {
    if raw.get("@type").and_then(Value::as_str) == Some("error") {
        return Err(Error::Protocol {
            provider: PROVIDER,
            message: raw
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("TDLib returned an error")
                .to_owned(),
        });
    }
    Ok(raw.clone())
}

fn messages_from(response: &Value, chat_id: i64, limit: usize) -> Result<Vec<Message>> {
    let messages = response
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol {
            provider: PROVIDER,
            message: "TDLib returned no `messages` array".to_owned(),
        })?;
    messages
        .iter()
        .take(limit)
        .map(|message| mapping::message(message, chat_id))
        .collect()
}

fn login_request(state: &Value, params: &Value) -> Option<Value> {
    let string = |key: &str| params.get(key).and_then(Value::as_str);
    match state.get("@type").and_then(Value::as_str)? {
        "authorizationStateWaitPhoneNumber" => Some(json!({
            "@type": "setAuthenticationPhoneNumber",
            "phone_number": string("phone")?,
        })),
        "authorizationStateWaitCode" => Some(json!({
            "@type": "checkAuthenticationCode",
            "code": string("code")?,
        })),
        "authorizationStateWaitPassword" => Some(json!({
            "@type": "checkAuthenticationPassword",
            "password": string("password")?,
        })),
        "authorizationStateWaitEmailAddress" => Some(json!({
            "@type": "setAuthenticationEmailAddress",
            "email_address": string("email")?,
        })),
        "authorizationStateWaitEmailCode" => Some(json!({
            "@type": "checkAuthenticationEmailCode",
            "code": { "@type": "emailAddressAuthenticationCode", "code": string("email_code")? },
        })),
        "authorizationStateWaitRegistration" => Some(json!({
            "@type": "registerUser",
            "first_name": string("first_name")?,
            "last_name": string("last_name").unwrap_or_default(),
        })),
        _ => None,
    }
}
