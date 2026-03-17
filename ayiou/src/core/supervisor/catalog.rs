use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;

use crate::core::model::EventEnvelope;

use super::types::{PluginConfigSnapshot, PluginHealth, RuntimeServices};

#[async_trait]
pub trait ManagedPlugin: Send + Sync + 'static {
    fn kind(&self) -> &'static str;

    async fn init(&mut self, _services: RuntimeServices) -> Result<()> {
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn handle_event(&self, _event: &EventEnvelope) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, _config: PluginConfigSnapshot) -> Result<()> {
        Ok(())
    }

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}

pub trait PluginFactory: Send + Sync + 'static {
    fn kind(&self) -> &'static str;
    fn create(&self, instance_id: &str) -> Box<dyn ManagedPlugin>;
}

#[derive(Clone, Default)]
pub struct PluginCatalog {
    factories: Arc<DashMap<String, Arc<dyn PluginFactory>>>,
}

impl PluginCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, factory: impl PluginFactory) {
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

    pub fn create(&self, kind: &str, instance_id: &str) -> Result<Box<dyn ManagedPlugin>> {
        let factory = self
            .factories
            .get(kind)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("plugin kind `{}` is not registered", kind))?;
        Ok(factory.create(instance_id))
    }
}
