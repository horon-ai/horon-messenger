//! A line-delimited JSON-RPC 2.0 client for providers that drive a child process.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use hyper_messenger_core::{Error, ProviderKind, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// The per-request timeout a [`JsonRpcProcess`] applies unless one is given.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A resolved sidecar child process: the program path, its arguments and the provider it serves.
#[derive(Debug, Clone)]
pub struct SidecarSpec {
    provider: ProviderKind,
    name: String,
    env_var: String,
    program: PathBuf,
    args: Vec<String>,
}

impl SidecarSpec {
    /// Builds a spec from an explicit program path and args, without resolving the path.
    pub fn new(provider: ProviderKind, program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            provider,
            name: String::new(),
            env_var: String::new(),
            program: program.into(),
            args,
        }
    }

    /// Resolves the program from `env_var` when it holds an executable path, else from `PATH`.
    pub fn resolve(
        provider: ProviderKind,
        env_var: &str,
        default_program: &str,
        args: Vec<String>,
    ) -> Result<Self> {
        let program = resolve_program(env_var, default_program).ok_or_else(|| Error::Config {
            message: format!(
                "sidecar binary `{default_program}` was not found; set `{env_var}` to its path"
            ),
        })?;
        Ok(Self {
            provider,
            name: default_program.to_owned(),
            env_var: env_var.to_owned(),
            program,
            args,
        })
    }

    /// The provider this sidecar serves.
    pub fn provider(&self) -> ProviderKind {
        self.provider
    }

    /// The default program name the spec was resolved from.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The environment variable consulted before the `PATH` lookup.
    pub fn env_var(&self) -> &str {
        &self.env_var
    }

    /// The resolved program path.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The arguments passed to the program.
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

fn resolve_program(env_var: &str, default_program: &str) -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(env_var)
        && !value.is_empty()
    {
        let path = PathBuf::from(value);
        return is_executable(&path).then_some(path);
    }
    which(default_program)
}

fn which(program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() || program.contains(std::path::MAIN_SEPARATOR) {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(program))
        .find(|full| is_executable(full))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

/// A spawned sidecar spoken to over line-delimited JSON-RPC 2.0 on stdin and stdout.
#[derive(Debug)]
pub struct JsonRpcProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    timeout: Duration,
    provider: ProviderKind,
}

impl JsonRpcProcess {
    /// Spawns the sidecar with the [`DEFAULT_TIMEOUT`] per-request budget.
    pub async fn start(spec: SidecarSpec) -> Result<Self> {
        Self::start_with_timeout(spec, DEFAULT_TIMEOUT).await
    }

    /// Spawns the sidecar with an explicit per-request timeout.
    pub async fn start_with_timeout(spec: SidecarSpec, timeout: Duration) -> Result<Self> {
        let provider = spec.provider();
        let mut command = Command::new(spec.program());
        command
            .args(spec.args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| Error::Network {
            provider,
            message: format!(
                "failed to spawn sidecar `{}`: {error}",
                spec.program().display()
            ),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| Error::Network {
            provider,
            message: "sidecar stdin pipe was not created".to_owned(),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| Error::Network {
            provider,
            message: "sidecar stdout pipe was not created".to_owned(),
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            timeout,
            provider,
        })
    }

    /// Sends a JSON-RPC request and returns the `result` correlated by its id.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let timeout = self.timeout;
        let provider = self.provider;
        tokio::time::timeout(timeout, async {
            self.write_message(&request).await?;
            self.read_response(id).await
        })
        .await
        .map_err(|_| Error::Network {
            provider,
            message: format!("sidecar request `{method}` timed out after {timeout:?}"),
        })?
    }

    /// Sends a fire-and-forget JSON-RPC notification that carries no id.
    pub async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let timeout = self.timeout;
        let provider = self.provider;
        tokio::time::timeout(timeout, self.write_message(&notification))
            .await
            .map_err(|_| Error::Network {
                provider,
                message: format!("sidecar notification `{method}` timed out after {timeout:?}"),
            })?
    }

    /// Kills the child process and reaps it.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        Ok(())
    }

    async fn write_message(&mut self, value: &Value) -> Result<()> {
        let mut line = serde_json::to_string(value).map_err(|error| Error::Protocol {
            provider: self.provider,
            message: format!("failed to encode a sidecar message: {error}"),
        })?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| Error::Network {
                provider: self.provider,
                message: format!("failed to write to the sidecar: {error}"),
            })?;
        self.stdin.flush().await.map_err(|error| Error::Network {
            provider: self.provider,
            message: format!("failed to flush the sidecar pipe: {error}"),
        })
    }

    async fn read_response(&mut self, id: u64) -> Result<Value> {
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|error| Error::Network {
                    provider: self.provider,
                    message: format!("failed to read from the sidecar: {error}"),
                })?;
            if read == 0 {
                return Err(Error::Network {
                    provider: self.provider,
                    message: "sidecar closed its stdout before responding".to_owned(),
                });
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value =
                serde_json::from_str(trimmed).map_err(|error| Error::Protocol {
                    provider: self.provider,
                    message: format!("sidecar returned non-JSON output: {error}"),
                })?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(Error::Protocol {
                    provider: self.provider,
                    message: format!("sidecar returned an error response: {error}"),
                });
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const ECHO_STUB: &str = "\
import sys, json
while True:
    line = sys.stdin.readline()
    if not line:
        break
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    if 'id' in req:
        resp = {'jsonrpc': '2.0', 'id': req['id'], 'result': {'echo': req.get('params')}}
        sys.stdout.write(json.dumps(resp) + '\\n')
        sys.stdout.flush()
";

    const SILENT_STUB: &str = "\
import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
";

    const GARBAGE_STUB: &str = "\
import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    sys.stdout.write('this is not json\\n')
    sys.stdout.flush()
";

    fn python_stub(script: &str) -> SidecarSpec {
        SidecarSpec::new(
            ProviderKind::Mock,
            "python3",
            vec!["-c".to_owned(), script.to_owned()],
        )
    }

    #[tokio::test]
    async fn request_response_round_trip() {
        let spec = SidecarSpec::resolve(
            ProviderKind::Mock,
            "HORON_SIDECAR_TEST_PYTHON",
            "python3",
            vec!["-c".to_owned(), ECHO_STUB.to_owned()],
        )
        .expect("python3 must resolve from PATH");
        assert_eq!(spec.name(), "python3");
        let mut process = JsonRpcProcess::start(spec).await.unwrap();
        let result = process
            .call("echo", serde_json::json!({ "hello": "world" }))
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({ "echo": { "hello": "world" } }));
        let second = process
            .call("echo", serde_json::json!([1, 2, 3]))
            .await
            .unwrap();
        assert_eq!(second, serde_json::json!({ "echo": [1, 2, 3] }));
        process.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn call_times_out_when_the_child_never_replies() {
        let mut process = JsonRpcProcess::start_with_timeout(
            python_stub(SILENT_STUB),
            Duration::from_millis(300),
        )
        .await
        .unwrap();
        let started = Instant::now();
        let error = process
            .call("echo", serde_json::json!({}))
            .await
            .unwrap_err();
        let elapsed = started.elapsed();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(
            elapsed < Duration::from_secs(5),
            "timeout must fire near the budget, took {elapsed:?}"
        );
        process.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn resolve_reports_a_missing_binary() {
        let error = SidecarSpec::resolve(
            ProviderKind::Mock,
            "HORON_SIDECAR_ABSENT_BINARY",
            "horon-definitely-not-a-real-binary",
            Vec::new(),
        )
        .unwrap_err();
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn resolve_rejects_an_env_var_that_points_nowhere() {
        unsafe { std::env::set_var("HORON_SIDECAR_BOGUS_PATH", "/no/such/binary") };
        let error = SidecarSpec::resolve(
            ProviderKind::Mock,
            "HORON_SIDECAR_BOGUS_PATH",
            "python3",
            Vec::new(),
        )
        .unwrap_err();
        unsafe { std::env::remove_var("HORON_SIDECAR_BOGUS_PATH") };
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn start_on_a_missing_binary_returns_a_typed_error() {
        let spec = SidecarSpec::new(
            ProviderKind::Mock,
            "/no/such/directory/horon-missing-binary",
            Vec::new(),
        );
        let error = JsonRpcProcess::start(spec).await.unwrap_err();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn non_json_output_is_a_protocol_error() {
        let mut process =
            JsonRpcProcess::start_with_timeout(python_stub(GARBAGE_STUB), Duration::from_secs(5))
                .await
                .unwrap();
        let error = process
            .call("echo", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Protocol { .. }), "got {error:?}");
        process.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn call_after_shutdown_returns_a_typed_error() {
        let mut process =
            JsonRpcProcess::start_with_timeout(python_stub(ECHO_STUB), Duration::from_millis(300))
                .await
                .unwrap();
        process.shutdown().await.unwrap();
        let started = Instant::now();
        let error = process
            .call("echo", serde_json::json!({}))
            .await
            .unwrap_err();
        let elapsed = started.elapsed();
        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(
            elapsed < Duration::from_secs(5),
            "a dead sidecar must fail fast, took {elapsed:?}"
        );
    }
}
