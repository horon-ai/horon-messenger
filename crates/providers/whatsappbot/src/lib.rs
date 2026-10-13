//! WhatsApp Bot provider. Every advertised operation returns `NotImplemented` until
//! the Cloud API client lands here.

use hyper_messenger_core::{
    Capabilities, ConfigPaths, FileCredentialStore, Provider, ProviderKind,
};

/// The WhatsApp Bot provider, authenticated with a bot number.
#[derive(Debug, Clone)]
pub struct WhatsAppBotProvider {
    paths: ConfigPaths,
    credentials: FileCredentialStore,
}

impl WhatsAppBotProvider {
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

impl Provider for WhatsAppBotProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::WhatsAppBot
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::WHOAMI | Capabilities::SEND_MESSAGE
    }
}
