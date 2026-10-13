//! iMessage provider, driven from the local Messages app.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The iMessage provider, driven from the local Messages app.
#[derive(Debug, Clone)]
pub struct IMessageProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl IMessageProvider {
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

impl Provider for IMessageProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::IMessage
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
