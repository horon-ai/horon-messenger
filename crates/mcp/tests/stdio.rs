//! Integration test that drives the built `horon-messenger-mcp` binary over stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

struct McpServer {
    child: Child,
    stdin: ChildStdin,
    frames: Receiver<Value>,
}

impl McpServer {
    fn spawn(config_dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_horon-messenger-mcp"))
            .env("HORON_MESSENGER_CONFIG_DIR", config_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the horon-messenger-mcp binary");
        let stdin = child.stdin.take().expect("child stdin is piped");
        let stdout = child.stdout.take().expect("child stdout is piped");

        let (sender, frames) = channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => break,
                };
                if line.trim().is_empty() {
                    continue;
                }
                let frame: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|error| panic!("server sent a non-JSON line ({error}): {line}"));
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });

        Self { child, stdin, frames }
    }

    fn send(&mut self, frame: Value) {
        let mut line = serde_json::to_string(&frame).expect("serialize the frame");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .expect("write the frame to the server");
        self.stdin.flush().expect("flush the frame");
    }

    fn response(&self, id: u64) -> Value {
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.frames.recv_timeout(remaining) {
                Ok(frame) => {
                    if frame.get("id").and_then(Value::as_u64) == Some(id) {
                        return frame;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    panic!("timed out after {RESPONSE_TIMEOUT:?} waiting for response id {id}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("server closed stdout before answering request id {id}")
                }
            }
        }
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct TempConfig {
    path: PathBuf,
}

impl TempConfig {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after the epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("horon-messenger-mcp-stdio-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the temp config dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn mcp_stdio_drives_initialize_and_tool_calls() {
    let config = TempConfig::new();
    let mut server = McpServer::spawn(config.path());

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "stdio-integration-test", "version": "0.0.0" }
        }
    }));
    let init = server.response(1);
    assert_eq!(
        init["result"]["serverInfo"]["name"], "horon-messenger",
        "initialize must report the server name: {init}"
    );

    server.send(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "list_providers", "arguments": {} }
    }));
    let providers = server.response(2);
    assert_eq!(
        providers["result"]["isError"].as_bool(),
        Some(false),
        "list_providers must succeed: {providers}"
    );
    let provider_list = providers["result"]["structuredContent"]["data"]
        .as_array()
        .unwrap_or_else(|| panic!("list_providers data is an array: {providers}"));
    assert!(
        provider_list.iter().any(|provider| provider["kind"] == "mock"),
        "list_providers must include the mock provider: {providers}"
    );

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "list_chats", "arguments": { "provider": "mock" } }
    }));
    let chats = server.response(3);
    assert_eq!(
        chats["result"]["isError"].as_bool(),
        Some(false),
        "list_chats must succeed: {chats}"
    );
    let chat_list = chats["result"]["structuredContent"]["data"]
        .as_array()
        .unwrap_or_else(|| panic!("list_chats data is an array: {chats}"));
    assert_eq!(chat_list.len(), 3, "the mock provider seeds three chats: {chats}");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "whoami", "arguments": { "provider": "discord" } }
    }));
    let whoami = server.response(4);
    assert_eq!(
        whoami["result"]["isError"].as_bool(),
        Some(true),
        "whoami for a provider without credentials must be a structured error: {whoami}"
    );
    assert_eq!(
        whoami["result"]["structuredContent"]["ok"].as_bool(),
        Some(false),
        "the failure envelope carries ok:false: {whoami}"
    );
    let code = whoami["result"]["structuredContent"]["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("the failure envelope carries an error code: {whoami}"));
    assert!(
        code == "NOT_REGISTERED" || code == "AUTH_REQUIRED",
        "whoami must fail because the provider is absent or unauthenticated, got {code}: {whoami}"
    );
}
