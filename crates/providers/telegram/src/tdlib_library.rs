//! Loading the native TDLib `tdjson` shared library and resolving its symbols.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int};
use std::path::PathBuf;

use horon_messenger_core::{Error, ProviderKind, Result};

type CreateClientId = unsafe extern "C" fn() -> c_int;
type SendRequest = unsafe extern "C" fn(c_int, *const c_char);
type ReceiveResponse = unsafe extern "C" fn(c_double) -> *const c_char;

const PROVIDER: ProviderKind = ProviderKind::Telegram;

struct Symbols {
    create_client_id: CreateClientId,
    send: SendRequest,
    receive: ReceiveResponse,
}

/// A loaded TDLib `tdjson` library with its client entry points resolved.
pub struct TdlibLibrary {
    _handle: Option<libloading::Library>,
    create_client_id: CreateClientId,
    send: SendRequest,
    receive: ReceiveResponse,
}

impl TdlibLibrary {
    /// Loads TDLib: from the current process when already linked, else from a default path.
    pub fn load() -> Result<Self> {
        if let Some(library) = Self::from_process() {
            return Ok(library);
        }
        Self::load_from(&default_candidates())
    }

    /// Loads TDLib by trying each candidate path in order.
    pub fn load_from(candidates: &[PathBuf]) -> Result<Self> {
        let mut failures = Vec::new();
        for candidate in candidates {
            match unsafe { libloading::Library::new(candidate) } {
                Ok(library) => match resolve(&library) {
                    Ok(symbols) => {
                        return Ok(Self {
                            _handle: Some(library),
                            create_client_id: symbols.create_client_id,
                            send: symbols.send,
                            receive: symbols.receive,
                        });
                    }
                    Err(error) => failures.push(format!("{}: {error}", candidate.display())),
                },
                Err(error) => failures.push(format!("{}: {error}", candidate.display())),
            }
        }
        Err(Error::Config {
            message: format!(
                "unable to load the TDLib shared library; set TDLIB_PATH to libtdjson, or place it on the library search path; tried: {}",
                if failures.is_empty() {
                    "no candidates".to_owned()
                } else {
                    failures.join("; ")
                }
            ),
        })
    }

    /// Creates a TDLib client id.
    pub fn create_client(&self) -> i32 {
        unsafe { (self.create_client_id)() }
    }

    /// Sends one JSON request to a TDLib client.
    pub fn send(&self, client_id: i32, request: &str) -> Result<()> {
        let encoded = CString::new(request).map_err(|_| Error::Protocol {
            provider: PROVIDER,
            message: "a TDLib request contained a NUL byte".to_owned(),
        })?;
        unsafe { (self.send)(client_id, encoded.as_ptr()) };
        Ok(())
    }

    /// Receives one JSON response, waiting up to `timeout` seconds.
    pub fn receive(&self, timeout: f64) -> Option<String> {
        let pointer = unsafe { (self.receive)(timeout) };
        if pointer.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned())
    }

    #[cfg(unix)]
    fn from_process() -> Option<Self> {
        let this = libloading::os::unix::Library::this();
        unsafe {
            let create_client_id = *this.get::<CreateClientId>(b"td_create_client_id").ok()?;
            let send = *this.get::<SendRequest>(b"td_send").ok()?;
            let receive = *this.get::<ReceiveResponse>(b"td_receive").ok()?;
            Some(Self {
                _handle: None,
                create_client_id,
                send,
                receive,
            })
        }
    }

    #[cfg(not(unix))]
    fn from_process() -> Option<Self> {
        None
    }
}

fn resolve(library: &libloading::Library) -> std::result::Result<Symbols, String> {
    unsafe {
        let create_client_id = *library
            .get::<CreateClientId>(b"td_create_client_id")
            .map_err(|error| format!("missing `td_create_client_id` ({error})"))?;
        let send = *library
            .get::<SendRequest>(b"td_send")
            .map_err(|error| format!("missing `td_send` ({error})"))?;
        let receive = *library
            .get::<ReceiveResponse>(b"td_receive")
            .map_err(|error| format!("missing `td_receive` ({error})"))?;
        Ok(Symbols {
            create_client_id,
            send,
            receive,
        })
    }
}

fn library_file_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libtdjson.dylib"
    } else if cfg!(target_os = "windows") {
        "tdjson.dll"
    } else {
        "libtdjson.so"
    }
}

fn default_candidates() -> Vec<PathBuf> {
    let name = library_file_name();
    let mut candidates = Vec::new();
    for var in ["TDLIB_PATH", "TDJSON_PATH", "HORON_MESSENGER_TDLIB_PATH"] {
        if let Some(value) = std::env::var_os(var)
            && !value.is_empty()
        {
            candidates.push(PathBuf::from(value));
        }
    }
    candidates.push(PathBuf::from(name));
    for dir in [
        "/opt/homebrew/lib",
        "/usr/local/lib",
        "/usr/lib",
        "/usr/lib/x86_64-linux-gnu",
    ] {
        candidates.push(PathBuf::from(dir).join(name));
    }
    candidates
}
