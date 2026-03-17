use std::sync::Arc;

use crate::core::{
    model::{BotId, PlatformId},
    observability::MetricsSink,
    plugin_host::OutboundSender,
    runtime::RuntimeStatus,
    scheduler::Scheduler,
    session::SessionStore,
    storage::Store,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginInstanceSpec {
    pub instance_id: String,
    pub kind: String,
    pub enabled: bool,
}

impl PluginInstanceSpec {
    pub fn new(instance_id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            kind: kind.into(),
            enabled: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotDefinition {
    pub bot_id: BotId,
    pub platform: PlatformId,
    pub adapter: String,
    pub plugins: Vec<PluginInstanceSpec>,
}

impl BotDefinition {
    pub fn new(
        bot_id: impl Into<BotId>,
        platform: impl Into<PlatformId>,
        adapter: impl Into<String>,
    ) -> Self {
        Self {
            bot_id: bot_id.into(),
            platform: platform.into(),
            adapter: adapter.into(),
            plugins: Vec::new(),
        }
    }

    pub fn with_plugin(mut self, plugin: PluginInstanceSpec) -> Self {
        self.plugins.push(plugin);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginConfigSnapshot {
    pub version: u64,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHealth {
    pub healthy: bool,
    pub detail: Option<String>,
}

impl PluginHealth {
    pub fn healthy() -> Self {
        Self {
            healthy: true,
            detail: None,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeServices {
    pub bot_id: BotId,
    pub platform: PlatformId,
    pub scheduler: Arc<dyn Scheduler>,
    pub store: Arc<dyn Store>,
    pub sessions: Arc<dyn SessionStore>,
    pub metrics: Arc<dyn MetricsSink>,
    pub outbound: Option<Arc<dyn OutboundSender>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotStatus {
    pub bot_id: BotId,
    pub platform: PlatformId,
    pub adapter: String,
    pub runtime: RuntimeStatus,
}
