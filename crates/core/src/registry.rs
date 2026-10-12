use std::collections::BTreeMap;
use std::fmt;

use crate::error::{Error, Result};
use crate::kind::ProviderKind;
use crate::provider::DynProvider;

/// The adapters a process compiled in, keyed by platform.
#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderKind, DynProvider>,
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.kinds()).finish()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: DynProvider) -> &mut Self {
        self.providers.insert(provider.kind(), provider);
        self
    }

    pub fn with(mut self, provider: DynProvider) -> Self {
        self.register(provider);
        self
    }

    pub fn get(&self, kind: ProviderKind) -> Option<DynProvider> {
        self.providers.get(&kind).cloned()
    }

    pub fn require(&self, kind: ProviderKind) -> Result<DynProvider> {
        self.get(kind)
            .ok_or(Error::NotRegistered { provider: kind })
    }

    pub fn kinds(&self) -> Vec<ProviderKind> {
        self.providers.keys().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ProviderKind, &DynProvider)> {
        self.providers
            .iter()
            .map(|(kind, provider)| (*kind, provider))
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
