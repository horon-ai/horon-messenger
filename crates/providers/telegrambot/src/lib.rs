//! Telegram Bot provider. Every advertised operation returns `NotImplemented` until
//! the Bot API client lands here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The Telegram Bot provider, authenticated with a bot token.
#[derive(Debug, Clone)]
pub struct TelegramBotProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl TelegramBotProvider {
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

impl Provider for TelegramBotProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::TelegramBot
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
