//! Spawns and drives the Node WhatsApp sidecar over line-delimited JSON-RPC.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hyper_messenger_core::{Error, ProviderKind, Result};
use hyper_messenger_provider_common::{JsonRpcProcess, SidecarSpec};
use serde_json::Value;

/// The environment variable holding the Node binary used to launch the sidecar.
pub const NODE_BIN_ENV: &str = "HYPER_MESSENGER_NODE_BIN";
/// The environment variable overriding the sidecar script path for installed layouts.
pub const SIDECAR_ENV: &str = "HYPER_MESSENGER_WHATSAPP_SIDECAR";
/// The environment variable overriding the per-request timeout in milliseconds.
pub const TIMEOUT_ENV: &str = "HYPER_MESSENGER_WHATSAPP_TIMEOUT_MS";
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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("sidecar")
        .join(SIDECAR_FILE)
}

fn request_timeout() -> Duration {
    let millis = std::env::var(TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    Duration::from_millis(millis)
}

/// A handle to the Node sidecar script that the provider spawns per request.
#[derive(Debug, Clone)]
pub struct Sidecar {
    script: PathBuf,
}

impl Sidecar {
    /// Builds a handle from the resolved script path.
    pub fn resolve() -> Self {
        Self {
            script: sidecar_script(),
        }
    }

    /// The resolved script path.
    #[cfg(test)]
    pub fn script(&self) -> &Path {
        &self.script
    }

    fn spec(&self) -> Result<SidecarSpec> {
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

    /// Starts the sidecar process with the resolved per-request timeout.
    pub async fn start(&self) -> Result<JsonRpcProcess> {
        JsonRpcProcess::start_with_timeout(self.spec()?, request_timeout()).await
    }

    /// Starts the sidecar, sends one request, and shuts the process down.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut process = self.start().await?;
        let result = process.call(method, params).await;
        let _ = process.shutdown().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_script_resolves_from_env_then_crate() {
        unsafe { std::env::remove_var(SIDECAR_ENV) };
        let default = sidecar_script();
        assert!(
            default.ends_with("sidecar/index.mjs"),
            "{}",
            default.display()
        );
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
        };
        let error = sidecar.spec().unwrap_err();
        assert!(matches!(error, Error::Config { .. }), "got {error:?}");
    }
}
