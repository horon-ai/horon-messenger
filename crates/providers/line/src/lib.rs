//! LINE provider, driven through a Node sidecar that wraps the linejs client.
//!
//! Implementation path (b), chosen over (a) a hand-rolled Thrift client: LINE's
//! user-account read/send surface is a Thrift service whose sessions are normally
//! established with an E2EE handshake, and no maintained Rust crate speaks it. Driving
//! the vendored linejs through `horon-messenger-provider-common`'s sidecar keeps this
//! crate pure Rust, keeps the LINE protocol and its E2EE handshake out of the
//! workspace, and lets the automated QA run against a stub program through
//! `HORON_MESSENGER_LINE_SIDECAR` the same way the operations run in production.
//!
//! Scope: whoami, list_chats, get_chat, list_messages, send_message, list_members,
//! snapshot. Out of scope: E2EE letter-sealing and the long-poll listener. Sending is
//! non-idempotent and is never retried automatically.

mod client;
mod models;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatKind, ChatQuery, ConfigPaths,
    CredentialStore, Delivery, Error, FileCredentialStore, LoginOptions, LoginOutcome, Member, Message, MessagePage,
    MessageQuery, Profile, Provider, ProviderKind, Result, SendRequest, SendResult, Snapshot, now_rfc3339,
};
use horon_messenger_provider_common::{DEFAULT_TIMEOUT, SidecarSpec};

use client::LineClient;
use models::{LineChat, LineMessage};

pub(crate) const PROVIDER: ProviderKind = ProviderKind::Line;

const BIN_ENV: &str = "HORON_MESSENGER_LINE_SIDECAR";
const DEFAULT_BIN: &str = "horon-messenger-line-sidecar";
const ACCOUNT_ENV: &str = "HORON_MESSENGER_LINE_ACCOUNT";
const DEFAULT_ACCOUNT: &str = "default";
const DEFAULT_LIMIT: usize = 50;
const CHAT_SCAN_LIMIT: usize = 1000;
const EPOCH: &str = "1970-01-01T00:00:00.000Z";

/// The LINE provider, driven through the linejs sidecar.
#[derive(Debug, Clone)]
pub struct LineProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    sidecar: Option<SidecarSpec>,
    timeout: Duration,
    session: Arc<Mutex<Option<LineClient>>>,
}

impl LineProvider {
    /// Builds a provider rooted at `paths`.
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            sidecar: None,
            timeout: DEFAULT_TIMEOUT,
            session: Arc::new(Mutex::new(None)),
        }
    }

    /// The config paths this provider reads credentials from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The credential store backing the stored accounts.
    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn spec(&self) -> Result<SidecarSpec> {
        match &self.sidecar {
            Some(spec) => Ok(spec.clone()),
            None => SidecarSpec::resolve(PROVIDER, BIN_ENV, DEFAULT_BIN, Vec::new()),
        }
    }

    fn current_account(&self) -> Result<String> {
        let accounts = self.credentials.list(PROVIDER)?;
        let explicit = std::env::var(ACCOUNT_ENV).ok().filter(|value| !value.is_empty());
        select_account(explicit.as_deref(), &accounts)?.ok_or(Error::AuthRequired { provider: PROVIDER })
    }

    async fn session(&self) -> Result<LineClient> {
        let mut guard = self.session.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = self.connect_and_login().await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    async fn connect_and_login(&self) -> Result<LineClient> {
        let account = self.current_account()?;
        let stored = self
            .credentials
            .load(PROVIDER, &account)?
            .ok_or(Error::AuthRequired { provider: PROVIDER })?;
        let token = stored
            .get("auth_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or(Error::AuthRequired { provider: PROVIDER })?
            .to_owned();
        let device = stored
            .get("device")
            .and_then(Value::as_str)
            .filter(|device| !device.is_empty())
            .map(str::to_owned);
        let mut client = LineClient::connect(self.spec()?, self.timeout).await?;
        let mut params = json!({ "token": token });
        if let Some(device) = device {
            params["device"] = json!(device);
        }
        client.login(params).await?;
        Ok(client)
    }

    async fn invalidate(&self) {
        let mut guard = self.session.lock().await;
        *guard = None;
    }

    fn chat(chat: &LineChat) -> Chat {
        Chat {
            provider: PROVIDER,
            id: chat.chat_id.clone(),
            name: if chat.display_name.is_empty() {
                chat.chat_id.clone()
            } else {
                chat.display_name.clone()
            },
            kind: chat_kind(&chat.chat_type),
            unread_count: None,
            last_message_at: None,
            member_count: chat.member_count,
            extra: json!({ "type": chat.chat_type, "picture_url": chat.picture_url }),
        }
    }

    fn message(chat_id: &str, message: &LineMessage) -> Message {
        Message {
            provider: PROVIDER,
            id: message.message_id.clone(),
            chat_id: if message.chat_id.is_empty() {
                chat_id.to_owned()
            } else {
                message.chat_id.clone()
            },
            sender_id: Some(message.author_id.clone()).filter(|id| !id.is_empty()),
            sender_name: message.author_name.clone().filter(|name| !name.is_empty()),
            text: message.text.clone().unwrap_or_default(),
            sent_at: message
                .sent_at
                .clone()
                .filter(|sent| !sent.is_empty())
                .unwrap_or_else(|| EPOCH.to_owned()),
            edited_at: None,
            attachments: Vec::new(),
            reply_to: None,
            extra: json!({ "content_type": message.content_type }),
        }
    }

    #[cfg(test)]
    fn for_test(spec: SidecarSpec, timeout: Duration, seed: bool) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("horon-messenger-line-tests-{}-{n}", std::process::id()));
        let paths = ConfigPaths::at(dir);
        let provider = Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            sidecar: Some(spec),
            timeout,
            session: Arc::new(Mutex::new(None)),
        };
        if seed {
            provider
                .credentials
                .save(
                    PROVIDER,
                    DEFAULT_ACCOUNT,
                    &json!({
                        "account_id": "u_self",
                        "auth_token": "test-token",
                        "device": "ANDROIDSECONDARY",
                        "display_name": "Self",
                        "method": "token",
                    }),
                )
                .unwrap();
        }
        provider
    }
}

#[async_trait]
impl Provider for LineProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
    }

    fn auth(&self) -> Option<&dyn AuthProvider> {
        Some(self)
    }

    async fn whoami(&self) -> Result<Profile> {
        let mut client = self.session().await?;
        let mut result = client.whoami().await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
            client = self.session().await?;
            result = client.whoami().await;
        }
        let profile = result?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: profile.mid,
            display_name: Some(profile.display_name).filter(|name| !name.is_empty()),
            username: None,
            extra: json!({
                "status_message": profile.status_message,
                "picture_url": profile.picture_url,
            }),
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        let mut client = self.session().await?;
        let mut result = client.list_chats(limit).await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
            client = self.session().await?;
            result = client.list_chats(limit).await;
        }
        let mut chats: Vec<Chat> = result?.iter().map(Self::chat).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| chat.name.to_lowercase().contains(&needle) || chat.id.to_lowercase().contains(&needle));
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        let mut client = self.session().await?;
        let mut result = client.list_chats(CHAT_SCAN_LIMIT).await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
            client = self.session().await?;
            result = client.list_chats(CHAT_SCAN_LIMIT).await;
        }
        result?
            .iter()
            .find(|chat| chat.chat_id == chat_id)
            .map(Self::chat)
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_id}"),
            })
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        let count = query.limit.unwrap_or(DEFAULT_LIMIT);
        let mut client = self.session().await?;
        let mut result = client.list_messages(chat_id, count).await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
            client = self.session().await?;
            result = client.list_messages(chat_id, count).await;
        }
        let messages = result?;
        let has_more = messages.len() >= count;
        let mut mapped: Vec<Message> = messages.iter().map(|message| Self::message(chat_id, message)).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            mapped.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages: mapped,
            has_more,
            newest_first: true,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        let mut client = self.session().await?;
        let result = client.send(&request.chat_id, &request.text).await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
        }
        let response = result?;
        let sent_at = response
            .sent_at
            .clone()
            .filter(|sent| !sent.is_empty())
            .unwrap_or_else(now_rfc3339);
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id: response.message_id.clone().filter(|id| !id.is_empty()),
            sent_at,
            accepted: response.success,
            delivery: Delivery::ServerAccepted,
            extra: json!({ "sidecar_chat_id": response.chat_id }),
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        let mut client = self.session().await?;
        let mut result = client.list_members(chat_id).await;
        if matches!(&result, Err(Error::Network { .. })) {
            self.invalidate().await;
            client = self.session().await?;
            result = client.list_members(chat_id).await;
        }
        Ok(result?
            .into_iter()
            .map(|member| Member {
                display_name: member
                    .display_name
                    .clone()
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| member.mid.clone()),
                id: member.mid,
                username: None,
                is_self: false,
                extra: Value::Null,
            })
            .collect())
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        let profile = self.whoami().await?;
        let chats = self.list_chats(&ChatQuery::default()).await?;
        Ok(Snapshot {
            provider: PROVIDER,
            profile: Some(profile),
            chats,
            generated_at: now_rfc3339(),
        })
    }
}

#[async_trait]
impl AuthProvider for LineProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[AuthMethod::Token, AuthMethod::Login]
    }

    async fn status(&self) -> Result<AuthStatus> {
        let accounts = self.credentials.list(PROVIDER)?;
        let current = self.current_account().ok();
        let summaries = accounts
            .iter()
            .map(|account| {
                let stored = self.credentials.load(PROVIDER, account).ok().flatten();
                AccountSummary {
                    account_id: account.clone(),
                    display_name: stored
                        .as_ref()
                        .and_then(|value| value.get("display_name"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    method: stored
                        .as_ref()
                        .and_then(|value| value.get("method"))
                        .and_then(Value::as_str)
                        .map(|method| match method {
                            "password" => AuthMethod::Login,
                            _ => AuthMethod::Token,
                        }),
                    current: current.as_deref() == Some(account.as_str()),
                    extra: Value::Null,
                }
            })
            .collect();
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated: !accounts.is_empty(),
            current,
            accounts: summaries,
        })
    }

    async fn login(&self, options: &LoginOptions) -> Result<LoginOutcome> {
        let key = options
            .account_id
            .clone()
            .filter(|account| !account.is_empty())
            .unwrap_or_else(|| DEFAULT_ACCOUNT.to_owned());
        let stored = self.credentials.load(PROVIDER, &key)?;
        let plan = plan_login(&options.params, stored.as_ref())?;
        let device = options
            .params
            .get("device")
            .and_then(Value::as_str)
            .filter(|device| !device.is_empty());
        let mut params = plan.params();
        if let Some(device) = device {
            params["device"] = json!(device);
        }
        if let Some(account) = options.account_id.clone().filter(|account| !account.is_empty()) {
            params["account_id"] = json!(account);
        }
        let mut client = LineClient::connect(self.spec()?, self.timeout).await?;
        let result = client.login(params).await;
        let _ = client.shutdown().await;
        let response = result?;
        if !response.authenticated {
            return Err(Error::AuthFailed {
                provider: PROVIDER,
                message: "LINE rejected the supplied credentials".to_owned(),
            });
        }
        let account_id = response
            .account_id
            .clone()
            .filter(|account| !account.is_empty())
            .or_else(|| options.account_id.clone())
            .unwrap_or_else(|| key.clone());
        let auth_token = response.auth_token.clone().or_else(|| plan.stored_token());
        let device_value = response.device.clone().or_else(|| device.map(str::to_owned));
        self.credentials.save(
            PROVIDER,
            &key,
            &json!({
                "account_id": account_id,
                "auth_token": auth_token,
                "device": device_value,
                "display_name": response.display_name,
                "method": plan.method(),
                "created_at": now_rfc3339(),
                "updated_at": now_rfc3339(),
            }),
        )?;
        Ok(LoginOutcome {
            provider: PROVIDER,
            authenticated: true,
            account_id: Some(account_id),
            next_action: None,
            message: response.display_name.map(|name| format!("signed in to LINE as {name}")),
        })
    }

    async fn logout(&self, account_id: Option<&str>) -> Result<()> {
        let key = match account_id.map(str::trim).filter(|account| !account.is_empty()) {
            Some(account) => account.to_owned(),
            None => self.current_account()?,
        };
        self.credentials.delete(PROVIDER, &key)
    }
}

fn chat_kind(raw: &str) -> ChatKind {
    match raw {
        "user" => ChatKind::Direct,
        "group" | "room" => ChatKind::Group,
        "square" => ChatKind::OpenChat,
        _ => ChatKind::Unknown,
    }
}

#[derive(Debug)]
enum LoginPlan {
    Token(String),
    Password { email: String, password: String },
}

impl LoginPlan {
    fn params(&self) -> Value {
        match self {
            LoginPlan::Token(token) => json!({ "token": token }),
            LoginPlan::Password { email, password } => {
                json!({ "email": email, "password": password })
            }
        }
    }

    fn method(&self) -> &'static str {
        match self {
            LoginPlan::Token(_) => "token",
            LoginPlan::Password { .. } => "password",
        }
    }

    fn stored_token(&self) -> Option<String> {
        match self {
            LoginPlan::Token(token) => Some(token.clone()),
            LoginPlan::Password { .. } => None,
        }
    }
}

fn plan_login(params: &Value, stored: Option<&Value>) -> Result<LoginPlan> {
    if let Some(token) = non_empty(params.get("token").and_then(Value::as_str)) {
        return Ok(LoginPlan::Token(token.to_owned()));
    }
    let email = non_empty(params.get("email").and_then(Value::as_str));
    let password = non_empty(params.get("password").and_then(Value::as_str));
    if let (Some(email), Some(password)) = (email, password) {
        return Ok(LoginPlan::Password {
            email: email.to_owned(),
            password: password.to_owned(),
        });
    }
    if let Some(token) = non_empty(stored.and_then(|value| value.get("auth_token")).and_then(Value::as_str)) {
        return Ok(LoginPlan::Token(token.to_owned()));
    }
    Err(Error::InvalidInput {
        message: "LINE login needs `token`, or `email` and `password`".to_owned(),
    })
}

fn select_account(explicit: Option<&str>, accounts: &[String]) -> Result<Option<String>> {
    if accounts.is_empty() {
        return Ok(None);
    }
    if let Some(name) = explicit {
        return if accounts.iter().any(|account| account == name) {
            Ok(Some(name.to_owned()))
        } else {
            Err(Error::NotFound {
                provider: PROVIDER,
                resource: format!("LINE account {name}"),
            })
        };
    }
    if accounts.iter().any(|account| account == DEFAULT_ACCOUNT) {
        return Ok(Some(DEFAULT_ACCOUNT.to_owned()));
    }
    if accounts.len() == 1 {
        return Ok(Some(accounts[0].clone()));
    }
    Err(Error::Ambiguous {
        provider: PROVIDER,
        resource: "LINE account".to_owned(),
        message: format!("{} accounts stored; set {ACCOUNT_ENV}", accounts.len()),
    })
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|trimmed| !trimmed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_STUB: &str = r#"
import sys, json
def emit(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def reply(req, result): emit({"jsonrpc": "2.0", "id": req.get("id"), "result": result})
def fail(req, code, message, data=None):
    e = {"code": code, "message": message}
    if data is not None: e["data"] = data
    emit({"jsonrpc": "2.0", "id": req.get("id"), "error": e})
CHATS = [
  {"chat_id": "u_friend", "type": "user", "display_name": "Friend", "member_count": 1},
  {"chat_id": "g_crew", "type": "group", "display_name": "Crew", "member_count": 3}
]
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    m = req.get("method")
    if m == "auth.login":
        reply(req, {"authenticated": True, "account_id": "u_self", "display_name": "Self", "device": "ANDROIDSECONDARY", "auth_token": "returned-token"})
    elif m == "whoami":
        reply(req, {"mid": "u_self", "display_name": "Self", "status_message": "hi", "picture_url": "https://profile.line-scdn.net/self.jpg"})
    elif m == "chats.list":
        reply(req, {"chats": CHATS})
    elif m == "messages.list":
        cid = req["params"]["chat_id"]
        reply(req, {"messages": [
          {"message_id": "m2", "chat_id": cid, "author_id": "u_self", "text": "yo", "content_type": "NONE", "sent_at": "2026-06-24T00:01:00.000Z"},
          {"message_id": "m1", "chat_id": cid, "author_id": "u_friend", "author_name": "Friend", "text": "hi", "content_type": "NONE", "sent_at": "2026-06-24T00:00:00.000Z"}
        ]})
    elif m == "members.list":
        reply(req, {"members": [{"mid": "u_friend", "display_name": "Friend"}, {"mid": "u_other"}]})
    elif m == "message.send":
        reply(req, {"success": True, "chat_id": req["params"]["chat_id"], "message_id": "sent-1", "sent_at": "2026-06-24T00:02:00.000Z"})
    else:
        fail(req, -32601, "Method not found")
"#;

    const AUTH_FAIL_STUB: &str = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "error": {"code": -32001, "message": "login failed", "data": "AUTH_FAILED: invalid token"}}) + "\n")
    sys.stdout.flush()
"#;

    const GARBAGE_STUB: &str = r#"
import sys
for line in sys.stdin:
    if not line: break
    sys.stdout.write("this is not json\n")
    sys.stdout.flush()
"#;

    const EMPTY_RESULT_STUB: &str = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "result": {}}) + "\n")
    sys.stdout.flush()
"#;

    const DEAD_STUB: &str = r#"
import sys
sys.exit(0)
"#;

    fn stub(script: &str) -> SidecarSpec {
        SidecarSpec::new(PROVIDER, "python3", vec!["-c".to_owned(), script.to_owned()])
    }

    fn provider(script: &str) -> LineProvider {
        LineProvider::for_test(stub(script), Duration::from_secs(5), true)
    }

    #[tokio::test]
    async fn maps_reference_chat_payload() {
        let chats = provider(FIXTURE_STUB).list_chats(&ChatQuery::default()).await.unwrap();
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].id, "u_friend");
        assert_eq!(chats[0].name, "Friend");
        assert_eq!(chats[0].kind, ChatKind::Direct);
        assert_eq!(chats[0].member_count, Some(1));
        assert_eq!(chats[0].extra["type"], "user");
        assert_eq!(chats[1].id, "g_crew");
        assert_eq!(chats[1].kind, ChatKind::Group);
        assert_eq!(chats[1].member_count, Some(3));
    }

    #[tokio::test]
    async fn maps_reference_message_payload_newest_first() {
        let page = provider(FIXTURE_STUB)
            .list_messages("u_friend", &MessageQuery::default())
            .await
            .unwrap();
        assert_eq!(page.chat_id, "u_friend");
        assert!(page.newest_first);
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].id, "m2");
        assert_eq!(page.messages[0].chat_id, "u_friend");
        assert_eq!(page.messages[0].sender_id.as_deref(), Some("u_self"));
        assert_eq!(page.messages[0].text, "yo");
        assert_eq!(page.messages[1].id, "m1");
        assert_eq!(page.messages[1].sender_name.as_deref(), Some("Friend"));
        assert_eq!(page.messages[1].text, "hi");
    }

    #[tokio::test]
    async fn get_chat_and_members_read_the_chat_payload() {
        let chat = provider(FIXTURE_STUB).get_chat("g_crew").await.unwrap();
        assert_eq!(chat.name, "Crew");
        assert_eq!(chat.kind, ChatKind::Group);
        let members = provider(FIXTURE_STUB).list_members("g_crew").await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].id, "u_friend");
        assert_eq!(members[0].display_name, "Friend");
        assert_eq!(members[1].id, "u_other");
        assert_eq!(members[1].display_name, "u_other");
    }

    #[tokio::test]
    async fn send_returns_the_real_message_id() {
        let request = SendRequest {
            chat_id: "u_friend".to_owned(),
            text: "hello".to_owned(),
            ..SendRequest::default()
        };
        let result = provider(FIXTURE_STUB).send_message(&request).await.unwrap();
        assert_eq!(result.chat_id, "u_friend");
        assert_eq!(result.message_id.as_deref(), Some("sent-1"));
        assert!(result.accepted);
        assert_eq!(result.delivery, Delivery::ServerAccepted);
        assert_eq!(result.sent_at, "2026-06-24T00:02:00.000Z");
    }

    #[tokio::test]
    async fn whoami_reports_the_profile() {
        let profile = provider(FIXTURE_STUB).whoami().await.unwrap();
        assert_eq!(profile.account_id, "u_self");
        assert_eq!(profile.display_name.as_deref(), Some("Self"));
        assert_eq!(profile.extra["status_message"], "hi");
    }

    #[tokio::test]
    async fn snapshot_combines_profile_and_chats() {
        let snapshot = provider(FIXTURE_STUB).snapshot().await.unwrap();
        assert_eq!(snapshot.profile.unwrap().account_id, "u_self");
        assert_eq!(snapshot.chats.len(), 2);
    }

    #[tokio::test]
    async fn login_stores_credentials_and_reports_the_account() {
        let provider = LineProvider::for_test(stub(FIXTURE_STUB), Duration::from_secs(5), false);
        let options = LoginOptions {
            account_id: None,
            method: None,
            params: json!({ "token": "QA_TOKEN" }),
        };
        let outcome = provider.login(&options).await.unwrap();
        assert!(outcome.authenticated);
        assert_eq!(outcome.account_id.as_deref(), Some("u_self"));
        let stored = provider.credentials().load(PROVIDER, DEFAULT_ACCOUNT).unwrap().unwrap();
        assert_eq!(stored["account_id"], "u_self");
        assert_eq!(stored["auth_token"], "returned-token");
        assert_eq!(stored["method"], "token");
    }

    #[tokio::test]
    async fn login_reports_an_auth_failure() {
        let provider = LineProvider::for_test(stub(AUTH_FAIL_STUB), Duration::from_secs(5), false);
        let options = LoginOptions {
            account_id: None,
            method: None,
            params: json!({ "token": "bad" }),
        };
        let error = provider.login(&options).await.unwrap_err();
        assert!(matches!(error, Error::AuthFailed { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn operations_map_a_rejected_token_to_auth_failed() {
        let error = provider(AUTH_FAIL_STUB)
            .list_chats(&ChatQuery::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::AuthFailed { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn operations_without_credentials_report_auth_required() {
        let provider = LineProvider::for_test(stub(FIXTURE_STUB), Duration::from_secs(5), false);
        let error = provider.list_chats(&ChatQuery::default()).await.unwrap_err();
        assert!(matches!(error, Error::AuthRequired { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn auth_status_lists_the_stored_account() {
        let status = provider(FIXTURE_STUB).status().await.unwrap();
        assert!(status.authenticated);
        assert_eq!(status.current.as_deref(), Some(DEFAULT_ACCOUNT));
        assert_eq!(status.accounts.len(), 1);
        assert_eq!(status.accounts[0].method, Some(AuthMethod::Token));
        assert!(status.accounts[0].current);
    }

    #[tokio::test]
    async fn non_json_output_is_a_protocol_error() {
        let error = provider(GARBAGE_STUB)
            .list_chats(&ChatQuery::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn a_payload_without_the_expected_fields_is_a_protocol_error() {
        let error = provider(EMPTY_RESULT_STUB)
            .list_chats(&ChatQuery::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn a_dead_sidecar_is_a_typed_error() {
        let error = provider(DEAD_STUB).list_chats(&ChatQuery::default()).await.unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
    }

    #[test]
    fn capabilities_cover_the_implemented_operations() {
        let capabilities = provider(FIXTURE_STUB).capabilities();
        for flag in [
            Capabilities::WHOAMI,
            Capabilities::LIST_CHATS,
            Capabilities::GET_CHAT,
            Capabilities::LIST_MESSAGES,
            Capabilities::SEND_MESSAGE,
            Capabilities::LIST_MEMBERS,
            Capabilities::SNAPSHOT,
        ] {
            assert!(capabilities.contains(flag), "missing {flag:?}");
        }
        assert!(!capabilities.contains(Capabilities::SEARCH_MESSAGES));
    }

    #[test]
    fn login_prefers_an_explicit_token_over_email_and_stored_token() {
        let stored = json!({ "auth_token": "stored" });
        let params = json!({ "token": "explicit", "email": "a@b.c", "password": "pw" });
        let plan = plan_login(&params, Some(&stored)).unwrap();
        assert_eq!(plan.method(), "token");
        assert_eq!(plan.stored_token().as_deref(), Some("explicit"));
    }

    #[test]
    fn login_uses_email_and_password_when_no_token_is_given() {
        let params = json!({ "email": "a@b.c", "password": "pw" });
        let plan = plan_login(&params, None).unwrap();
        assert_eq!(plan.method(), "password");
        assert_eq!(plan.params(), json!({ "email": "a@b.c", "password": "pw" }));
    }

    #[test]
    fn login_falls_back_to_the_stored_token() {
        let stored = json!({ "auth_token": "stored" });
        let plan = plan_login(&json!({}), Some(&stored)).unwrap();
        assert_eq!(plan.method(), "token");
        assert_eq!(plan.stored_token().as_deref(), Some("stored"));
    }

    #[test]
    fn login_without_credentials_is_invalid_input() {
        let error = plan_login(&json!({}), None).unwrap_err();
        assert!(matches!(error, Error::InvalidInput { .. }), "got {error:?}");
    }

    #[test]
    fn account_selection_prefers_the_default_then_a_single_account() {
        let accounts = vec!["u_a".to_owned(), DEFAULT_ACCOUNT.to_owned()];
        assert_eq!(
            select_account(None, &accounts).unwrap().as_deref(),
            Some(DEFAULT_ACCOUNT)
        );
        let single = vec!["u_only".to_owned()];
        assert_eq!(select_account(None, &single).unwrap().as_deref(), Some("u_only"));
        assert_eq!(select_account(None, &[]).unwrap(), None);
    }

    #[test]
    fn account_selection_honors_an_explicit_name_and_rejects_unknown() {
        let accounts = vec!["u_a".to_owned(), "u_b".to_owned()];
        assert_eq!(select_account(Some("u_b"), &accounts).unwrap().as_deref(), Some("u_b"));
        let error = select_account(Some("u_missing"), &accounts).unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }), "got {error:?}");
    }

    #[test]
    fn account_selection_is_ambiguous_with_multiple_accounts() {
        let accounts = vec!["u_a".to_owned(), "u_b".to_owned()];
        let error = select_account(None, &accounts).unwrap_err();
        assert!(matches!(error, Error::Ambiguous { .. }), "got {error:?}");
    }

    const STUB: &str = r#"
import sys, os, json
pidfile = sys.argv[1]
limit = int(sys.argv[2])
with open(pidfile, 'a') as f:
    f.write(str(os.getpid()) + '\n')
answered = 0
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    m = req.get('method')
    if m == 'auth.login':
        result = {'authenticated': True, 'account_id': 'u_self', 'display_name': 'Self', 'device': 'ANDROIDSECONDARY', 'auth_token': 'returned-token'}
    elif m == 'chats.list':
        result = {'chats': []}
    else:
        result = {}
    sys.stdout.write(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}) + '\n')
    sys.stdout.flush()
    answered += 1
    if limit and answered >= limit:
        break
"#;

    fn stub_spec(pid_file: &std::path::Path, limit: u32) -> SidecarSpec {
        SidecarSpec::new(
            PROVIDER,
            "python3",
            vec![
                "-c".to_owned(),
                STUB.to_owned(),
                pid_file.to_string_lossy().into_owned(),
                limit.to_string(),
            ],
        )
    }

    fn temp_pid_file(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("hm-line-sidecar-{name}-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn recorded_pids(path: &std::path::Path) -> Vec<u32> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect()
    }

    fn process_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    async fn wait_until_dead(pid: u32) {
        for _ in 0..500 {
            if !process_alive(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("child process {pid} was still alive after the drop");
    }

    #[tokio::test]
    async fn sidecar_process_is_reused() {
        let pid_file = temp_pid_file("reused");
        let provider = LineProvider::for_test(stub_spec(&pid_file, 0), Duration::from_secs(5), true);
        for _ in 0..3 {
            provider.list_chats(&ChatQuery::default()).await.unwrap();
        }
        let pids = recorded_pids(&pid_file);
        assert_eq!(
            pids.len(),
            1,
            "three operations must reuse one sidecar process: {pids:?}"
        );
        drop(provider);
        let _ = std::fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn sidecar_restarts_after_death() {
        let pid_file = temp_pid_file("restart");
        let provider = LineProvider::for_test(stub_spec(&pid_file, 2), Duration::from_secs(5), true);
        provider.list_chats(&ChatQuery::default()).await.unwrap();
        let first = recorded_pids(&pid_file);
        assert_eq!(first.len(), 1, "the first call starts one process: {first:?}");

        provider.list_chats(&ChatQuery::default()).await.unwrap();
        let after = recorded_pids(&pid_file);
        assert!(
            after.len() >= 2,
            "the call after a death must start a new process: {after:?}"
        );
        assert_ne!(
            after[after.len() - 1],
            first[0],
            "the restarted process must have a new pid: {after:?}"
        );
        drop(provider);
        let _ = std::fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn sidecar_child_is_reaped_on_drop() {
        let pid_file = temp_pid_file("reaped");
        let provider = LineProvider::for_test(stub_spec(&pid_file, 0), Duration::from_secs(5), true);
        provider.list_chats(&ChatQuery::default()).await.unwrap();
        let pids = recorded_pids(&pid_file);
        assert_eq!(pids.len(), 1, "expected one live process: {pids:?}");
        let child = pids[0];
        drop(provider);
        wait_until_dead(child).await;
        let _ = std::fs::remove_file(&pid_file);
    }
}
