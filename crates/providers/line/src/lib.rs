//! LINE provider. Every advertised operation returns `NotImplemented` until the LINE client lands here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The LINE provider: account authentication over the LINE client.
#[derive(Debug, Clone)]
pub struct LineProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl LineProvider {
    /// Builds a provider rooted at `paths`.
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
        }
    }

    /// The config paths this provider reads credentials from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The credential store backing the `default` account.
    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }
}

impl Provider for LineProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Line
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
}
