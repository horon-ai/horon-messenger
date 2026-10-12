//! Telegram provider. Every advertised operation returns `NotImplemented` until
//! the TDLib or Bot API client lands here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

#[derive(Debug, Clone)]
pub struct TelegramProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl TelegramProvider {
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

impl Provider for TelegramProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Telegram
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEARCH_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::SEND_MEDIA
            | Capabilities::MARK_READ
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
            | Capabilities::LISTEN
    }
}
