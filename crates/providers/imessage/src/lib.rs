//! iMessage provider, driven from the local Messages app through the `imsg` sidecar.

mod client;
mod models;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use horon_messenger_core::{
    AccountSummary, AuthMethod, AuthProvider, AuthStatus, Capabilities, Chat, ChatKind, ChatQuery, ConfigPaths, Error,
    FileCredentialStore, Member, Message, MessagePage, MessageQuery, Profile, Provider, ProviderKind, Result,
    SendRequest, SendResult, Snapshot, now_rfc3339,
};
use horon_messenger_provider_common::{DEFAULT_TIMEOUT, SidecarSpec};

use client::IMessageClient;
use models::{ImsgChat, ImsgMessage};

pub(crate) const PROVIDER: ProviderKind = ProviderKind::IMessage;

const DEFAULT_BIN: &str = "imsg";
const BIN_ENV: &str = "HORON_MESSENGER_IMSG_BIN";
const LOCAL_ACCOUNT: &str = "local";
const CHAT_SCAN_LIMIT: usize = 1000;
const DEFAULT_LIMIT: usize = 25;
const EPOCH: &str = "1970-01-01T00:00:00.000Z";

/// The iMessage provider, driven from the local Messages app.
#[derive(Debug, Clone)]
pub struct IMessageProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
    sidecar: Option<SidecarSpec>,
    timeout: Duration,
    client: Arc<Mutex<Option<IMessageClient>>>,
}

impl IMessageProvider {
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            sidecar: None,
            timeout: DEFAULT_TIMEOUT,
            client: Arc::new(Mutex::new(None)),
        }
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }

    fn spec(&self) -> Result<SidecarSpec> {
        match &self.sidecar {
            Some(spec) => Ok(spec.clone()),
            None => SidecarSpec::resolve(PROVIDER, BIN_ENV, DEFAULT_BIN, vec!["rpc".to_owned()]),
        }
    }

    async fn client(&self) -> Result<IMessageClient> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = IMessageClient::connect(self.spec()?, self.timeout).await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    async fn invalidate_if_dead<T>(&self, result: &Result<T>) {
        if matches!(result, Err(Error::Network { .. })) {
            let mut guard = self.client.lock().await;
            *guard = None;
        }
    }

    fn chat(chat: &ImsgChat) -> Chat {
        let participants = chat.participants.clone().unwrap_or_default();
        let name = chat
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| chat.identifier.clone())
            .or_else(|| chat.guid.clone())
            .unwrap_or_else(|| chat.id.to_string());
        Chat {
            provider: PROVIDER,
            id: chat.id.to_string(),
            name,
            kind: if chat.is_group {
                ChatKind::Group
            } else {
                ChatKind::Direct
            },
            unread_count: None,
            last_message_at: chat.last_message.as_ref().and_then(|m| m.created_at.clone()),
            member_count: chat.participants.as_ref().map(|p| p.len() as u64),
            extra: json!({
                "guid": chat.guid,
                "identifier": chat.identifier,
                "service": chat.service.clone().unwrap_or_else(|| "iMessage".to_owned()),
                "is_group": chat.is_group,
                "participants": participants,
            }),
        }
    }

    fn message(message: &ImsgMessage) -> Message {
        let sender_id = if message.is_from_me {
            None
        } else {
            message.sender.clone().filter(|sender| !sender.is_empty())
        };
        let id = message
            .guid
            .clone()
            .filter(|guid| !guid.is_empty())
            .unwrap_or_else(|| message.id.to_string());
        Message {
            provider: PROVIDER,
            id,
            chat_id: message.chat_id.to_string(),
            sender_id,
            sender_name: message.sender_name.clone(),
            text: message.text.clone().unwrap_or_default(),
            sent_at: message.created_at.clone().unwrap_or_else(|| EPOCH.to_owned()),
            edited_at: None,
            attachments: Vec::new(),
            reply_to: None,
            extra: json!({ "rowid": message.id, "is_outgoing": message.is_from_me }),
        }
    }

    fn matches(chat: &ImsgChat, chat_ref: &str) -> bool {
        chat.id.to_string() == chat_ref
            || chat.guid.as_deref() == Some(chat_ref)
            || chat.identifier.as_deref() == Some(chat_ref)
    }

    async fn resolve_chat_id(client: &mut IMessageClient, chat_ref: &str) -> Result<u64> {
        if let Ok(id) = chat_ref.parse::<u64>() {
            return Ok(id);
        }
        let chats = client.list_chats(CHAT_SCAN_LIMIT).await?;
        chats
            .iter()
            .find(|chat| Self::matches(chat, chat_ref))
            .map(|chat| chat.id)
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_ref}"),
            })
    }

    fn send_params(chat_ref: &str, text: &str) -> Value {
        let mut params = json!({ "text": text });
        if let Ok(id) = chat_ref.parse::<u64>() {
            params["chat_id"] = json!(id);
        } else if chat_ref.contains(';') {
            params["chat_guid"] = json!(chat_ref);
        } else {
            params["to"] = json!(chat_ref);
        }
        params
    }

    fn ensure_macos(operation: &'static str) -> Result<()> {
        if cfg!(target_os = "macos") {
            Ok(())
        } else {
            Err(Error::Unsupported {
                provider: PROVIDER,
                operation,
            })
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    fn for_test(spec: SidecarSpec, timeout: Duration) -> Self {
        let paths = ConfigPaths::at(std::env::temp_dir().join("horon-messenger-imessage-tests"));
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
            sidecar: Some(spec),
            timeout,
            client: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Provider for IMessageProvider {
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
        Self::ensure_macos("whoami")?;
        let spec = self.spec()?;
        let binary = spec.program().display().to_string();
        let mut client = self.client().await?;
        let probe = client.list_chats(1).await;
        self.invalidate_if_dead(&probe).await;
        probe?;
        Ok(Profile {
            provider: PROVIDER,
            account_id: LOCAL_ACCOUNT.to_owned(),
            display_name: Some("iMessage (local Messages app)".to_owned()),
            username: None,
            extra: json!({ "binary_path": binary, "login": "not required" }),
        })
    }

    async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
        Self::ensure_macos("list_chats")?;
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        let mut client = self.client().await?;
        let result = client.list_chats(limit).await;
        self.invalidate_if_dead(&result).await;
        let mut chats: Vec<Chat> = result?.iter().map(Self::chat).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            chats.retain(|chat| {
                chat.name.to_lowercase().contains(&needle)
                    || chat
                        .extra
                        .get("identifier")
                        .and_then(Value::as_str)
                        .is_some_and(|identifier| identifier.to_lowercase().contains(&needle))
            });
        }
        Ok(chats)
    }

    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        Self::ensure_macos("get_chat")?;
        let mut client = self.client().await?;
        let result = client.list_chats(CHAT_SCAN_LIMIT).await;
        self.invalidate_if_dead(&result).await;
        let chats = result?;
        chats
            .iter()
            .find(|chat| Self::matches(chat, chat_id))
            .map(Self::chat)
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_id}"),
            })
    }

    async fn list_messages(&self, chat_id: &str, query: &MessageQuery) -> Result<MessagePage> {
        Self::ensure_macos("list_messages")?;
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        let mut client = self.client().await?;
        let outcome = async {
            let id = Self::resolve_chat_id(&mut client, chat_id).await?;
            let mut messages = client.history(id, limit, query.after.as_deref()).await?;
            messages.sort_by(|a, b| {
                a.created_at
                    .as_deref()
                    .unwrap_or(EPOCH)
                    .cmp(b.created_at.as_deref().unwrap_or(EPOCH))
            });
            Ok::<_, Error>(messages)
        }
        .await;
        self.invalidate_if_dead(&outcome).await;
        let messages = outcome?;
        let has_more = messages.len() >= limit;
        let mut mapped: Vec<Message> = messages.iter().map(Self::message).collect();
        if let Some(search) = &query.search {
            let needle = search.to_lowercase();
            mapped.retain(|message| message.text.to_lowercase().contains(&needle));
        }
        Ok(MessagePage {
            provider: PROVIDER,
            chat_id: chat_id.to_owned(),
            messages: mapped,
            has_more,
            newest_first: false,
        })
    }

    async fn send_message(&self, request: &SendRequest) -> Result<SendResult> {
        Self::ensure_macos("send_message")?;
        let params = Self::send_params(&request.chat_id, &request.text);
        let mut client = self.client().await?;
        let result = client.send(params).await;
        self.invalidate_if_dead(&result).await;
        let response = result?;
        let message_id = response
            .guid
            .filter(|guid| !guid.is_empty())
            .or_else(|| response.id.map(|id| id.to_string()));
        Ok(SendResult {
            provider: PROVIDER,
            chat_id: request.chat_id.clone(),
            message_id,
            sent_at: now_rfc3339(),
            accepted: response.ok.unwrap_or(true),
            delivery: horon_messenger_core::Delivery::ServerAccepted,
            extra: json!({ "service": response.service }),
        })
    }

    async fn list_members(&self, chat_id: &str) -> Result<Vec<Member>> {
        Self::ensure_macos("list_members")?;
        let mut client = self.client().await?;
        let result = client.list_chats(CHAT_SCAN_LIMIT).await;
        self.invalidate_if_dead(&result).await;
        let chats = result?;
        let chat = chats
            .iter()
            .find(|chat| Self::matches(chat, chat_id))
            .ok_or_else(|| Error::NotFound {
                provider: PROVIDER,
                resource: format!("chat {chat_id}"),
            })?;
        Ok(chat
            .participants
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|participant| Member {
                id: participant.clone(),
                display_name: participant,
                username: None,
                is_self: false,
                extra: Value::Null,
            })
            .collect())
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        Self::ensure_macos("snapshot")?;
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
impl AuthProvider for IMessageProvider {
    fn kind(&self) -> ProviderKind {
        PROVIDER
    }

    fn methods(&self) -> &'static [AuthMethod] {
        &[]
    }

    async fn status(&self) -> Result<AuthStatus> {
        Ok(AuthStatus {
            provider: PROVIDER,
            authenticated: true,
            current: Some(LOCAL_ACCOUNT.to_owned()),
            accounts: vec![AccountSummary {
                account_id: LOCAL_ACCOUNT.to_owned(),
                display_name: Some("iMessage (local Messages app)".to_owned()),
                method: None,
                current: true,
                extra: json!({ "note": "no login needed; iMessage uses the local Messages app" }),
            }],
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use horon_messenger_core::Delivery;

    const FIXTURE_STUB: &str = r#"
import sys, json
def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n"); sys.stdout.flush()
CHATS = [
  {"id": 42, "name": "Jane", "identifier": "iMessage;-;+15551234567", "guid": "iMessage;-;+15551234567", "service": "iMessage", "is_group": False, "participants": ["+15551234567"], "last_message": {"id": 1, "chat_id": 42, "guid": "m1", "sender": "+15551234567", "is_from_me": False, "text": "hi", "created_at": "2026-06-24T00:00:00.000Z"}},
  {"id": 43, "name": "Crew", "identifier": "iMessage;+;chat99", "guid": "iMessage;+;chat99", "service": "iMessage", "is_group": True, "participants": ["+15551111111", "+15552222222"]}
]
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    m = req.get("method")
    if m == "chats.list":
        emit({"jsonrpc": "2.0", "id": req["id"], "result": {"chats": CHATS}})
    elif m == "messages.history":
        cid = req["params"]["chat_id"]
        emit({"jsonrpc": "2.0", "id": req["id"], "result": {"messages": [
          {"id": 2, "chat_id": cid, "guid": "m2", "sender": "", "is_from_me": True, "text": "yo", "created_at": "2026-06-24T00:01:00.000Z"},
          {"id": 1, "chat_id": cid, "guid": "m1", "sender": "+15551234567", "is_from_me": False, "text": "hi", "created_at": "2026-06-24T00:00:00.000Z"}
        ]}})
    elif m == "send":
        emit({"jsonrpc": "2.0", "id": req["id"], "result": {"ok": True, "id": 99, "guid": "sent-guid", "service": "iMessage"}})
    else:
        emit({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32601, "message": "Method not found"}})
"#;

    const FDA_STUB: &str = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32603, "message": "Internal error", "data": "Permission Error: Cannot access Messages database"}}) + "\n")
    sys.stdout.flush()
"#;

    const AUTOMATION_STUB: &str = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32603, "message": "Internal error", "data": "AppleScript error: Messages got an error: Not authorized to send Apple events"}}) + "\n")
    sys.stdout.flush()
"#;

    const SILENT_STUB: &str = r#"
import sys, time
time.sleep(3600)
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
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req["id"], "result": {}}) + "\n")
    sys.stdout.flush()
"#;

    const DEAD_STUB: &str = r#"
import sys
sys.exit(0)
"#;

    fn provider(script: &str) -> IMessageProvider {
        IMessageProvider::for_test(
            SidecarSpec::new(PROVIDER, "python3", vec!["-c".to_owned(), script.to_owned()]),
            Duration::from_secs(5),
        )
    }

    #[tokio::test]
    async fn maps_reference_chats_payload() {
        let chats = provider(FIXTURE_STUB).list_chats(&ChatQuery::default()).await.unwrap();
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].id, "42");
        assert_eq!(chats[0].name, "Jane");
        assert_eq!(chats[0].kind, ChatKind::Direct);
        assert_eq!(chats[0].member_count, Some(1));
        assert_eq!(chats[0].last_message_at.as_deref(), Some("2026-06-24T00:00:00.000Z"));
        assert_eq!(chats[0].extra["service"], "iMessage");
        assert_eq!(chats[1].id, "43");
        assert_eq!(chats[1].kind, ChatKind::Group);
        assert_eq!(chats[1].member_count, Some(2));
        assert_eq!(chats[1].last_message_at, None);
    }

    #[tokio::test]
    async fn maps_reference_messages_payload_oldest_first() {
        let page = provider(FIXTURE_STUB)
            .list_messages("42", &MessageQuery::default())
            .await
            .unwrap();
        assert_eq!(page.chat_id, "42");
        assert!(!page.newest_first);
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].id, "m1");
        assert_eq!(page.messages[0].chat_id, "42");
        assert_eq!(page.messages[0].sender_id.as_deref(), Some("+15551234567"));
        assert_eq!(page.messages[0].text, "hi");
        assert_eq!(page.messages[1].id, "m2");
        assert_eq!(page.messages[1].sender_id, None);
        assert_eq!(page.messages[1].extra["is_outgoing"], true);
    }

    #[tokio::test]
    async fn resolves_a_chat_guid_to_its_row_id() {
        let page = provider(FIXTURE_STUB)
            .list_messages("iMessage;+;chat99", &MessageQuery::default())
            .await
            .unwrap();
        assert_eq!(page.messages[0].chat_id, "43");
    }

    #[tokio::test]
    async fn get_chat_and_members_read_the_chat_payload() {
        let chat = provider(FIXTURE_STUB).get_chat("43").await.unwrap();
        assert_eq!(chat.name, "Crew");
        let members = provider(FIXTURE_STUB).list_members("43").await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].id, "+15551111111");
        assert_eq!(members[0].display_name, "+15551111111");
    }

    #[tokio::test]
    async fn send_maps_a_numeric_target_and_returns_the_guid() {
        let request = SendRequest {
            chat_id: "42".to_owned(),
            text: "hello".to_owned(),
            ..SendRequest::default()
        };
        let result = provider(FIXTURE_STUB).send_message(&request).await.unwrap();
        assert_eq!(result.chat_id, "42");
        assert_eq!(result.message_id.as_deref(), Some("sent-guid"));
        assert!(result.accepted);
        assert_eq!(result.delivery, Delivery::ServerAccepted);
    }

    #[tokio::test]
    async fn whoami_probes_the_database_and_reports_the_binary() {
        let profile = provider(FIXTURE_STUB).whoami().await.unwrap();
        assert_eq!(profile.account_id, "local");
        assert_eq!(profile.extra["login"], "not required");
    }

    #[tokio::test]
    async fn auth_status_reports_no_login_needed() {
        let status = provider(FIXTURE_STUB).status().await.unwrap();
        assert!(status.authenticated);
        assert_eq!(status.current.as_deref(), Some("local"));
        assert_eq!(status.accounts.len(), 1);
        assert_eq!(status.accounts[0].method, None);
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

    #[tokio::test]
    async fn missing_binary_is_a_typed_error() {
        let provider = IMessageProvider::for_test(
            SidecarSpec::new(PROVIDER, "/nonexistent/horon-messenger-imsg", vec!["rpc".to_owned()]),
            Duration::from_secs(5),
        );
        let error = provider.list_chats(&ChatQuery::default()).await.unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn full_disk_access_denial_names_the_permission() {
        let error = provider(FDA_STUB).list_chats(&ChatQuery::default()).await.unwrap_err();
        match error {
            Error::AuthFailed { message, .. } => {
                assert!(message.contains("Full Disk Access"), "got {message}");
            }
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn automation_denial_names_the_permission() {
        let request = SendRequest {
            chat_id: "42".to_owned(),
            text: "hello".to_owned(),
            ..SendRequest::default()
        };
        let error = provider(AUTOMATION_STUB).send_message(&request).await.unwrap_err();
        match error {
            Error::AuthFailed { message, .. } => {
                assert!(message.contains("Automation"), "got {message}");
            }
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hung_sidecar_times_out() {
        let provider = IMessageProvider::for_test(
            SidecarSpec::new(PROVIDER, "python3", vec!["-c".to_owned(), SILENT_STUB.to_owned()]),
            Duration::from_millis(300),
        );
        let error = provider.list_chats(&ChatQuery::default()).await.unwrap_err();
        match error {
            Error::Network { message, .. } => {
                assert!(message.contains("timed out"), "got {message}")
            }
            other => panic!("expected Network, got {other:?}"),
        }
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
    if m == 'chats.list':
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
        let path = std::env::temp_dir().join(format!("hm-imessage-sidecar-{name}-{}.txt", std::process::id()));
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
        let provider = IMessageProvider::for_test(stub_spec(&pid_file, 0), Duration::from_secs(5));
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
        let provider = IMessageProvider::for_test(stub_spec(&pid_file, 1), Duration::from_secs(5));
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
        let provider = IMessageProvider::for_test(stub_spec(&pid_file, 0), Duration::from_secs(5));
        provider.list_chats(&ChatQuery::default()).await.unwrap();
        let pids = recorded_pids(&pid_file);
        assert_eq!(pids.len(), 1, "expected one live process: {pids:?}");
        let child = pids[0];
        drop(provider);
        wait_until_dead(child).await;
        let _ = std::fs::remove_file(&pid_file);
    }
}
