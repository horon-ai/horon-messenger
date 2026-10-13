//! Instagram provider. Every advertised operation returns `NotImplemented` until the Instagram client lands here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The Instagram provider: account authentication over the Instagram client.
#[derive(Debug, Clone)]
pub struct InstagramProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl InstagramProvider {
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

impl Provider for InstagramProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Instagram
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEARCH_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
    }
}
