//! WhatsApp provider, driven from a linked WhatsApp account.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The WhatsApp provider, driven from a linked WhatsApp account.
#[derive(Debug, Clone)]
pub struct WhatsAppProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl WhatsAppProvider {
    pub fn new(paths: ConfigPaths) -> Self {
        Self {
            credentials: FileCredentialStore::new(paths.clone()),
            paths,
        }
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn credentials(&self) -> &FileCredentialStore {
        &self.credentials
    }
}

impl Provider for WhatsAppProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::WhatsApp
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
