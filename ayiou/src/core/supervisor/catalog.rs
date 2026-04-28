use std::sync::Arc;

use anyhow::{Result, anyhow};
use dashmap::DashMap;

use crate::core::{context::Context, plugin_system::RuntimePluginFactory};

#[derive(Clone, Default)]
pub struct PluginCatalog {
    factories: Arc<DashMap<String, Arc<dyn RuntimePluginFactory<Context>>>>,
}

impl PluginCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, factory: impl RuntimePluginFactory<Context>) {
        self.factories
            .insert(factory.kind().to_string(), Arc::new(factory));
    }

    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<_> = self
            .factories
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        kinds.sort();
        kinds
    }

    pub fn create(
        &self,
        kind: &str,
        instance_id: &str,
    ) -> Result<Box<dyn crate::core::plugin_system::RuntimePlugin<Context>>> {
        let factory = self
            .factories
            .get(kind)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("plugin kind `{}` is not registered", kind))?;
        factory.create(instance_id)
    }
}
