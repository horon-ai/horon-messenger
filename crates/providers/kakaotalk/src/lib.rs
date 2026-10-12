//! KakaoTalk provider. The production implementation moves here from
//! `~/Desktop/dev/alpox/kakaotalk-bridge`; until then every advertised operation
//! returns `NotImplemented`.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

#[derive(Debug, Clone)]
pub struct KakaoTalkProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl KakaoTalkProvider {
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

impl Provider for KakaoTalkProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::KakaoTalk
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
    }
}
