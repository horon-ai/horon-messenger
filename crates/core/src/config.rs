use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::kind::ProviderKind;

pub const CONFIG_DIR_ENV: &str = "HORON_MESSENGER_CONFIG_DIR";
pub const APP_DIR_NAME: &str = "horon-messenger";

/// Where per-provider credentials and state live. One environment variable
/// (`HORON_MESSENGER_CONFIG_DIR`) relocates everything for sandboxes and CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    root: PathBuf,
}

impl ConfigPaths {
    pub fn resolve() -> Result<Self> {
        if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV).filter(|value| !value.is_empty()) {
            return Ok(Self::at(PathBuf::from(dir)));
        }
        let base = dirs::config_dir().ok_or_else(|| Error::Config {
            message: format!("cannot determine a config directory; set {CONFIG_DIR_ENV}"),
        })?;
        Ok(Self::at(base.join(APP_DIR_NAME)))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn provider_dir(&self, provider: ProviderKind) -> PathBuf {
        self.root.join(provider.as_str())
    }

    pub fn accounts_dir(&self, provider: ProviderKind) -> PathBuf {
        self.provider_dir(provider).join("accounts")
    }

    pub fn credentials_file(&self, provider: ProviderKind) -> PathBuf {
        self.provider_dir(provider).join("credentials.json")
    }

    pub fn state_file(&self, provider: ProviderKind, name: &str) -> PathBuf {
        self.provider_dir(provider).join(format!("{name}.json"))
    }

    pub fn ensure_provider_dir(&self, provider: ProviderKind) -> Result<PathBuf> {
        let dir = self.provider_dir(provider);
        create_private_dir(&dir)?;
        Ok(dir)
    }
}

/// Create a directory tree readable only by the owner (`0700` on unix).
pub fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write a file atomically, owner-only (`0600` on unix): a temp file in the same
/// directory is written and renamed, so a reader never sees a partial file.
pub fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| Error::Config {
        message: format!("{} has no parent directory", path.display()),
    })?;
    create_private_dir(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    std::fs::write(&temp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temp, path)?;
    Ok(())
}
