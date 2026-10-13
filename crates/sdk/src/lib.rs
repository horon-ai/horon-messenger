pub use hyper_messenger_core as core;
pub use hyper_messenger_core::*;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

/// The one entry point every surface uses: a registry of the providers this build
/// enabled, plus the config paths their credentials live under.
#[derive(Debug, Clone)]
pub struct Client {
    registry: ProviderRegistry,
    paths: ConfigPaths,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn provider(&self, kind: ProviderKind) -> Result<DynProvider> {
        self.registry.require(kind)
    }

    pub fn describe(&self) -> Vec<ProviderInfo> {
        self.registry
            .iter()
            .map(|(kind, provider)| ProviderInfo::new(kind, provider.capabilities()))
            .collect()
    }

    pub async fn whoami(&self, kind: ProviderKind) -> Result<Profile> {
        self.provider(kind)?.whoami().await
    }

    pub async fn list_chats(&self, kind: ProviderKind, query: &ChatQuery) -> Result<Vec<Chat>> {
        self.provider(kind)?.list_chats(query).await
    }

    pub async fn list_messages(
        &self,
        kind: ProviderKind,
        chat_id: &str,
        query: &MessageQuery,
    ) -> Result<MessagePage> {
        self.provider(kind)?.list_messages(chat_id, query).await
    }

    pub async fn send_message(
        &self,
        kind: ProviderKind,
        request: &SendRequest,
    ) -> Result<SendResult> {
        self.provider(kind)?.send_message(request).await
    }

    pub async fn list_members(&self, kind: ProviderKind, chat_id: &str) -> Result<Vec<Member>> {
        self.provider(kind)?.list_members(chat_id).await
    }

    pub async fn snapshot(&self, kind: ProviderKind) -> Result<Snapshot> {
        self.provider(kind)?.snapshot().await
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub kind: ProviderKind,
    pub name: &'static str,
    pub description: &'static str,
    pub capabilities: Vec<&'static str>,
}

impl ProviderInfo {
    fn new(kind: ProviderKind, capabilities: Capabilities) -> Self {
        Self {
            kind,
            name: kind.display_name(),
            description: kind.description(),
            capabilities: capabilities.names(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ClientBuilder {
    registry: ProviderRegistry,
    paths: Option<ConfigPaths>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(mut self, provider: DynProvider) -> Self {
        self.registry.register(provider);
        self
    }

    pub fn config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.paths = Some(ConfigPaths::at(dir));
        self
    }

    pub fn config_paths(mut self, paths: ConfigPaths) -> Self {
        self.paths = Some(paths);
        self
    }

    pub fn build(mut self) -> Result<Client> {
        let paths = match self.paths.take() {
            Some(paths) => paths,
            None => ConfigPaths::resolve()?,
        };
        register_builtin(&mut self.registry, &paths);
        Ok(Client {
            registry: self.registry,
            paths,
        })
    }
}

fn register_builtin(registry: &mut ProviderRegistry, paths: &ConfigPaths) {
    #[cfg(feature = "mock")]
    {
        registry.register(Arc::new(hyper_messenger_provider_mock::MockProvider::new()));
    }
    #[cfg(feature = "kakaotalk")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_kakaotalk::KakaoTalkProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "discord")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_discord::DiscordProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "telegram")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_telegram::TelegramProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "channeltalk")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_channeltalk::ChannelTalkProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "telegrambot")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_telegrambot::TelegramBotProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "whatsappbot")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_whatsappbot::WhatsAppBotProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "slackbot")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_slackbot::SlackBotProvider::new(paths.clone()),
        ));
    }
    #[cfg(feature = "slack")]
    {
        registry.register(Arc::new(
            hyper_messenger_provider_slack::SlackProvider::new(paths.clone()),
        ));
    }
    let _ = paths;
}

/// A client with every provider this build enabled, resolving the default config
/// directory.
pub fn default_client() -> Result<Client> {
    Client::builder().build()
}
