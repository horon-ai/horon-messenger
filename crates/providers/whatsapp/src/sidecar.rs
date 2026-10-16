//! Spawns and drives the Node WhatsApp sidecar over line-delimited JSON-RPC.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use horon_messenger_core::{Error, ProviderKind, Result};
use horon_messenger_provider_common::{SidecarSession, SidecarSpec};
use serde_json::Value;

/// The environment variable holding the Node binary used to launch the sidecar.
pub const NODE_BIN_ENV: &str = "HORON_MESSENGER_NODE_BIN";
/// The environment variable overriding the sidecar script path for installed layouts.
pub const SIDECAR_ENV: &str = "HORON_MESSENGER_WHATSAPP_SIDECAR";
/// The environment variable overriding the per-request timeout in milliseconds.
pub const TIMEOUT_ENV: &str = "HORON_MESSENGER_WHATSAPP_TIMEOUT_MS";
/// The default per-request timeout for a sidecar call.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

const PROVIDER: ProviderKind = ProviderKind::WhatsApp;
const DEFAULT_NODE: &str = "node";
const SIDECAR_FILE: &str = "index.mjs";

/// Resolves the sidecar script: the env override, else the crate-local build path.
pub fn sidecar_script() -> PathBuf {
    if let Some(value) = std::env::var_os(SIDECAR_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("sidecar").join(SIDECAR_FILE)
}

fn request_timeout() -> Duration {
    let millis = std::env::var(TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    Duration::from_millis(millis)
}

/// A handle to the Node sidecar script that reuses one child process across requests.
#[derive(Debug, Clone)]
pub struct Sidecar {
    script: PathBuf,
    session: Arc<Mutex<Option<SidecarSession>>>,
    #[cfg(test)]
    spec_override: Option<SidecarSpec>,
}

impl Sidecar {
    /// Builds a handle from the resolved script path.
    pub fn resolve() -> Self {
        Self {
            script: sidecar_script(),
            session: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            spec_override: None,
        }
    }

    /// The resolved script path.
    #[cfg(test)]
    pub fn script(&self) -> &Path {
        &self.script
    }

    #[cfg(test)]
    fn for_test(spec: SidecarSpec) -> Self {
        Self {
            script: PathBuf::from(spec.program()),
            session: Arc::new(Mutex::new(None)),
            spec_override: Some(spec),
        }
    }

    fn spec(&self) -> Result<SidecarSpec> {
        #[cfg(test)]
        if let Some(spec) = &self.spec_override {
            return Ok(spec.clone());
        }
        if !self.script.is_file() {
            return Err(Error::Config {
                message: format!(
                    "WhatsApp sidecar script was not found at {}; run `npm ci` in the sidecar directory or set {SIDECAR_ENV}",
                    self.script.display()
                ),
            });
        }
        let args = vec![self.script.to_string_lossy().into_owned()];
        SidecarSpec::resolve(PROVIDER, NODE_BIN_ENV, DEFAULT_NODE, args)
    }

    fn session(&self) -> Result<SidecarSession> {
        let mut guard = self.session.lock().expect("the sidecar mutex was poisoned");
        if guard.is_none() {
            *guard = Some(SidecarSession::new(self.spec()?, request_timeout()));
        }
        Ok(guard.as_ref().expect("the sidecar session was just built").clone())
    }

    /// Sends one request over the reused sidecar process.
    ///
    /// `idempotent` marks a read-only call that may be retried after the child
    /// dies; the send path passes `false` so a possibly-applied message is never
    /// sent twice.
    pub async fn request(&self, method: &str, params: Value, idempotent: bool) -> Result<Value> {
        let session = self.session()?;
        session.call(method, params, idempotent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn stub_spec(pid_file: &Path, limit: u32) -> SidecarSpec {
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

    fn temp_pid_file(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("hm-wa-sidecar-{name}-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn recorded_pids(path: &Path) -> Vec<u32> {
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
        let sidecar = Sidecar::for_test(stub_spec(&pid_file, 0));
        for _ in 0..3 {
            sidecar
                .request("chats.list", serde_json::json!({}), true)
                .await
                .unwrap();
        }
        let pids = recorded_pids(&pid_file);
        assert_eq!(pids.len(), 1, "three requests must reuse one sidecar process: {pids:?}");
        drop(sidecar);
        let _ = std::fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn sidecar_restarts_after_death() {
        let pid_file = temp_pid_file("restart");
        let sidecar = Sidecar::for_test(stub_spec(&pid_file, 1));
        sidecar
            .request("chats.list", serde_json::json!({}), true)
            .await
            .unwrap();
        let first = recorded_pids(&pid_file);
        assert_eq!(first.len(), 1, "the first call starts one process: {first:?}");

        sidecar
            .request("chats.list", serde_json::json!({}), true)
            .await
            .unwrap();
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
        drop(sidecar);
        let _ = std::fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn sidecar_child_is_reaped_on_drop() {
        let pid_file = temp_pid_file("reaped");
        let sidecar = Sidecar::for_test(stub_spec(&pid_file, 0));
        sidecar
            .request("chats.list", serde_json::json!({}), true)
            .await
            .unwrap();
        let pids = recorded_pids(&pid_file);
        assert_eq!(pids.len(), 1, "expected one live process: {pids:?}");
        let child = pids[0];
        drop(sidecar);
        wait_until_dead(child).await;
        let _ = std::fs::remove_file(&pid_file);
    }

    #[test]
    fn sidecar_script_resolves_from_env_then_crate() {
        unsafe { std::env::remove_var(SIDECAR_ENV) };
        let default = sidecar_script();
        assert!(default.ends_with("sidecar/index.mjs"), "{}", default.display());
        assert!(default.is_file(), "{}", default.display());

        let dir = std::env::temp_dir().join(format!("hm-whatsapp-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let external = dir.join("index.mjs");
        std::fs::write(&external, "process.stdin.resume()\n").unwrap();
        unsafe { std::env::set_var(SIDECAR_ENV, &external) };
        assert_eq!(sidecar_script(), external);
        assert_eq!(Sidecar::resolve().script(), external);
        unsafe { std::env::remove_var(SIDECAR_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_sidecar_script_is_a_config_error() {
        let sidecar = Sidecar {
            script: PathBuf::from("/no/such/whatsapp-sidecar/index.mjs"),
            session: Arc::new(Mutex::new(None)),
            spec_override: None,
        };
        let error = sidecar.spec().unwrap_err();
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");
    }
}
