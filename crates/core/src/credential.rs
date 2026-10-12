use serde_json::Value;

use crate::config::{ConfigPaths, write_private_atomic};
use crate::error::{Error, Result};
use crate::kind::ProviderKind;

/// Per-account credential storage. A provider chooses the file layout through
/// [`FileCredentialStore`]; a test can substitute an in-memory store.
pub trait CredentialStore: Send + Sync + 'static {
    fn load(&self, provider: ProviderKind, account_id: &str) -> Result<Option<Value>>;
    fn save(&self, provider: ProviderKind, account_id: &str, value: &Value) -> Result<()>;
    fn list(&self, provider: ProviderKind) -> Result<Vec<String>>;
    fn delete(&self, provider: ProviderKind, account_id: &str) -> Result<()>;
}

/// A [`CredentialStore`] backed by `<config>/<provider>/accounts/<account>.json`,
/// owner-only on disk.
#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    paths: ConfigPaths,
}

impl FileCredentialStore {
    pub fn new(paths: ConfigPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    fn account_file(&self, provider: ProviderKind, account_id: &str) -> Result<std::path::PathBuf> {
        let safe = !account_id.is_empty()
            && !account_id.contains('/')
            && !account_id.contains('\\')
            && account_id != "..";
        if !safe {
            return Err(Error::InvalidInput {
                message: format!("invalid account id `{account_id}`"),
            });
        }
        Ok(self
            .paths
            .accounts_dir(provider)
            .join(format!("{account_id}.json")))
    }
}

impl CredentialStore for FileCredentialStore {
    fn load(&self, provider: ProviderKind, account_id: &str) -> Result<Option<Value>> {
        let path = self.account_file(provider, account_id)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Io(error)),
        }
    }

    fn save(&self, provider: ProviderKind, account_id: &str, value: &Value) -> Result<()> {
        let path = self.account_file(provider, account_id)?;
        let bytes = serde_json::to_vec_pretty(value)?;
        write_private_atomic(&path, &bytes)
    }

    fn list(&self, provider: ProviderKind) -> Result<Vec<String>> {
        let dir = self.paths.accounts_dir(provider);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Error::Io(error)),
        };
        let mut accounts = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(account_id) = name.strip_suffix(".json") {
                accounts.push(account_id.to_owned());
            }
        }
        accounts.sort();
        Ok(accounts)
    }

    fn delete(&self, provider: ProviderKind, account_id: &str) -> Result<()> {
        let path = self.account_file(provider, account_id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }
}
