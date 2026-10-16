//! Background control of the HTTP API server as a detached process.

use std::fs;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use horon_messenger_sdk::{ConfigPaths, Error, Result, now_rfc3339};
use serde::{Deserialize, Serialize};

/// The binary that runs the HTTP API server, and the variable that overrides
/// where the CLI looks for it.
const SERVER_BIN: &str = "horon-messenger-server";
const SERVER_BIN_ENV: &str = "HORON_MESSENGER_SERVER";

const PID_FILE: &str = "server.pid";
const LOG_FILE: &str = "server.log";
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(150);

/// The pidfile contents: enough to report status and stop the process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub started_at: String,
    pub log: String,
}

/// What `daemon status` reports.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

impl DaemonStatus {
    fn stopped() -> Self {
        Self {
            running: false,
            host: None,
            port: None,
            pid: None,
            started_at: None,
            url: None,
            log: None,
        }
    }

    fn from_state(state: &DaemonState, running: bool) -> Self {
        Self {
            running,
            host: Some(state.host.clone()),
            port: Some(state.port),
            pid: running.then_some(state.pid),
            started_at: running.then_some(state.started_at.clone()),
            url: Some(format!("http://{}:{}", state.host, state.port)),
            log: Some(state.log.clone()),
        }
    }
}

/// Start the server in the background and wait until it accepts connections.
pub fn start(paths: &ConfigPaths, host: &str, port: u16) -> Result<DaemonStatus> {
    if let Some(state) = read_state(paths) {
        if probe(&state.host, state.port) {
            return Err(Error::InvalidInput {
                message: format!(
                    "the server is already running (pid {}) on http://{}:{}",
                    state.pid, state.host, state.port
                ),
            });
        }
        remove_state(paths);
    }

    let exe = find_server()?;
    let log = log_path(paths);
    if let Some(parent) = log.parent() {
        horon_messenger_sdk::config::create_private_dir(parent)?;
    }
    let stdout = fs::OpenOptions::new().create(true).append(true).open(&log)?;
    let stderr = stdout.try_clone()?;

    let mut command = Command::new(&exe);
    command
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .arg("--config-dir")
        .arg(paths.root())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach(&mut command);

    let child = command.spawn()?;
    let state = DaemonState {
        pid: child.id(),
        host: host.to_owned(),
        port,
        started_at: now_rfc3339(),
        log: log.display().to_string(),
    };
    write_state(paths, &state)?;

    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if probe(host, port) {
            return Ok(DaemonStatus::from_state(&state, true));
        }
        if !is_alive(state.pid) {
            remove_state(paths);
            return Err(Error::Internal {
                message: format!("the server exited during startup; see {}", state.log),
            });
        }
        std::thread::sleep(POLL);
    }
    Err(Error::Internal {
        message: format!(
            "the server did not become ready within {}s; see {}",
            READY_TIMEOUT.as_secs(),
            state.log
        ),
    })
}

/// Stop the background server, if one is running.
pub fn stop(paths: &ConfigPaths) -> Result<DaemonStatus> {
    let Some(state) = read_state(paths) else {
        return Ok(DaemonStatus::stopped());
    };
    if !is_alive(state.pid) && !probe(&state.host, state.port) {
        remove_state(paths);
        return Ok(DaemonStatus::stopped());
    }
    terminate(state.pid)?;
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if !probe(&state.host, state.port) {
            remove_state(paths);
            return Ok(DaemonStatus::stopped());
        }
        std::thread::sleep(POLL);
    }
    Err(Error::Internal {
        message: format!(
            "the server (pid {}) did not stop within {}s",
            state.pid,
            READY_TIMEOUT.as_secs()
        ),
    })
}

/// Restart the background server.
pub fn restart(paths: &ConfigPaths, host: &str, port: u16) -> Result<DaemonStatus> {
    stop(paths)?;
    start(paths, host, port)
}

/// Report whether the server is running, using the pidfile and a live probe.
pub fn status(paths: &ConfigPaths) -> DaemonStatus {
    match read_state(paths) {
        Some(state) if probe(&state.host, state.port) => DaemonStatus::from_state(&state, true),
        Some(state) => DaemonStatus::from_state(&state, false),
        None => DaemonStatus::stopped(),
    }
}

fn pid_path(paths: &ConfigPaths) -> PathBuf {
    paths.root().join(PID_FILE)
}

fn log_path(paths: &ConfigPaths) -> PathBuf {
    paths.root().join(LOG_FILE)
}

fn read_state(paths: &ConfigPaths) -> Option<DaemonState> {
    let bytes = fs::read(pid_path(paths)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_state(paths: &ConfigPaths, state: &DaemonState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(state)?;
    horon_messenger_sdk::config::write_private_atomic(&pid_path(paths), &bytes)
}

fn remove_state(paths: &ConfigPaths) {
    let _ = fs::remove_file(pid_path(paths));
}

fn probe(host: &str, port: u16) -> bool {
    match format!("{host}:{port}").to_socket_addrs() {
        Ok(addresses) => addresses
            .into_iter()
            .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(400)).is_ok()),
        Err(_) => false,
    }
}

fn find_server() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os(SERVER_BIN_ENV) {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(Error::Config {
            message: format!("{SERVER_BIN_ENV} points to {} which is not a file", path.display()),
        });
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(server_exe_name());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if let Some(path) = which(server_exe_name()) {
        return Ok(path);
    }
    Err(Error::Config {
        message: format!("could not find `{SERVER_BIN}`; install the server package or set {SERVER_BIN_ENV}"),
    })
}

fn server_exe_name() -> &'static str {
    if cfg!(windows) {
        "horon-messenger-server.exe"
    } else {
        SERVER_BIN
    }
}

/// Resolve an executable by name on `PATH`, appending `.exe` on Windows.
pub(crate) fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        if !name.ends_with(".exe") {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn is_alive(_pid: u32) -> bool {
    true
}

#[cfg(unix)]
fn terminate(pid: u32) -> Result<()> {
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        Err(Error::Io(io::Error::last_os_error()))
    }
}

#[cfg(windows)]
fn terminate(pid: u32) -> Result<()> {
    let pid = pid.to_string();
    let status = Command::new("taskkill").args(["/PID", pid.as_str()]).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Internal {
            message: format!("taskkill failed for pid {pid}"),
        })
    }
}

#[cfg(not(any(unix, windows)))]
fn terminate(_pid: u32) -> Result<()> {
    Err(Error::Internal {
        message: "stopping the daemon is not supported on this platform".to_owned(),
    })
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut Command) {}
