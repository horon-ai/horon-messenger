//! Channel Talk provider. Every advertised operation returns `NotImplemented`
//! until the operator and API-credential clients land here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

#[derive(Debug, Clone)]
pub struct ChannelTalkProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl ChannelTalkProvider {
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

impl Provider for ChannelTalkProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::ChannelTalk
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI
            | Capabilities::LIST_CHATS
            | Capabilities::GET_CHAT
            | Capabilities::LIST_MESSAGES
            | Capabilities::SEND_MESSAGE
            | Capabilities::SEND_MEDIA
            | Capabilities::LIST_MEMBERS
            | Capabilities::SNAPSHOT
    }
}
