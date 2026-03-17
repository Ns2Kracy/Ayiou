use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::Mutex;

use crate::core::{
    config_store::{ConfigRecord, ConfigStore},
    observability::{MetricsSink, NoopMetrics},
    plugin_host::OutboundSender,
    plugin_runtime::{PluginInstanceState, PluginLifecycleState, PluginRuntimeState},
    runtime::{RuntimeController, RuntimeState},
    scheduler::{Scheduler, TokioScheduler},
    session::{MemorySessionStore, SessionStore},
    storage::{MemoryStore, Store},
};

use super::{
    catalog::{ManagedPlugin, PluginCatalog},
    types::{BotDefinition, BotStatus, PluginConfigSnapshot, PluginInstanceSpec, RuntimeServices},
};

impl From<ConfigRecord> for PluginConfigSnapshot {
    fn from(value: ConfigRecord) -> Self {
        Self {
            version: value.version,
            content: value.content,
        }
    }
}

struct PluginSlot {
    spec: PluginInstanceSpec,
    plugin: Mutex<Option<Box<dyn ManagedPlugin>>>,
}

struct BotRecord {
    definition: BotDefinition,
    runtime: RuntimeController,
    plugins: DashMap<String, Arc<PluginSlot>>,
    plugin_state: PluginRuntimeState,
    services: RuntimeServices,
}

#[derive(Clone)]
pub struct Supervisor {
    catalog: PluginCatalog,
    config_store: Arc<dyn ConfigStore>,
    bots: Arc<DashMap<String, Arc<BotRecord>>>,
    scheduler: Arc<dyn Scheduler>,
    store: Arc<dyn Store>,
    sessions: Arc<dyn SessionStore>,
    metrics: Arc<dyn MetricsSink>,
    outbound: Option<Arc<dyn OutboundSender>>,
}

impl Supervisor {
    pub fn new(config_store: Arc<dyn ConfigStore>) -> Self {
        Self {
            catalog: PluginCatalog::new(),
            config_store,
            bots: Arc::new(DashMap::new()),
            scheduler: Arc::new(TokioScheduler::new()),
            store: Arc::new(MemoryStore::new()),
            sessions: Arc::new(MemorySessionStore::new()),
            metrics: Arc::new(NoopMetrics),
            outbound: None,
        }
    }

    pub fn with_catalog(mut self, catalog: PluginCatalog) -> Self {
        self.catalog = catalog;
        self
    }

    pub fn with_scheduler(mut self, scheduler: Arc<dyn Scheduler>) -> Self {
        self.scheduler = scheduler;
        self
    }

    pub fn with_store(mut self, store: Arc<dyn Store>) -> Self {
        self.store = store;
        self
    }

    pub fn with_sessions(mut self, sessions: Arc<dyn SessionStore>) -> Self {
        self.sessions = sessions;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsSink>) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn with_outbound(mut self, outbound: Arc<dyn OutboundSender>) -> Self {
        self.outbound = Some(outbound);
        self
    }

    fn bot_record(&self, bot_id: &str) -> Result<Arc<BotRecord>> {
        self.bots
            .get(bot_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("bot `{}` is not registered", bot_id))
    }

    fn plugin_slot(&self, record: &Arc<BotRecord>, instance_id: &str) -> Result<Arc<PluginSlot>> {
        record
            .plugins
            .get(instance_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("plugin instance `{}` is not registered", instance_id))
    }

    async fn prepare_plugin_instance(
        &self,
        record: &Arc<BotRecord>,
        instance_id: &str,
        config: Option<PluginConfigSnapshot>,
        ensure_running: bool,
    ) -> Result<()> {
        if !record.plugin_state.is_enabled(instance_id) && ensure_running && config.is_none() {
            record
                .plugin_state
                .set_lifecycle(instance_id, PluginLifecycleState::Stopped);
            return Ok(());
        }

        let slot = self.plugin_slot(record, instance_id)?;
        let mut plugin_guard = slot.plugin.lock().await;
        let created = if plugin_guard.is_none() {
            record
                .plugin_state
                .set_lifecycle(instance_id, PluginLifecycleState::Initializing);
            let mut plugin = self.catalog.create(&slot.spec.kind, instance_id)?;
            plugin.init(record.services.clone()).await?;
            *plugin_guard = Some(plugin);
            true
        } else {
            false
        };

        let plugin = plugin_guard
            .as_mut()
            .ok_or_else(|| anyhow!("plugin instance `{}` is unavailable", instance_id))?;

        if let Some(config) = config {
            plugin.apply_config(config.clone()).await?;
            record
                .plugin_state
                .mark_config_applied(instance_id, config.version);
        }

        if ensure_running && record.plugin_state.is_enabled(instance_id) {
            if created {
                record
                    .plugin_state
                    .set_lifecycle(instance_id, PluginLifecycleState::Starting);
                plugin.start().await?;
            }
            record
                .plugin_state
                .set_lifecycle(instance_id, PluginLifecycleState::Running);
        } else if created {
            record
                .plugin_state
                .set_lifecycle(instance_id, PluginLifecycleState::Stopped);
        }

        record.plugin_state.clear_error(instance_id);
        Ok(())
    }

    async fn ensure_plugin_running(
        &self,
        record: &Arc<BotRecord>,
        instance_id: &str,
    ) -> Result<()> {
        self.prepare_plugin_instance(record, instance_id, None, true)
            .await
    }

    async fn stop_plugin_instance(&self, record: &Arc<BotRecord>, instance_id: &str) -> Result<()> {
        let slot = self.plugin_slot(record, instance_id)?;
        let mut plugin_guard = slot.plugin.lock().await;

        if let Some(plugin) = plugin_guard.as_mut() {
            record
                .plugin_state
                .set_lifecycle(instance_id, PluginLifecycleState::Stopping);
            plugin.stop().await?;
        }

        *plugin_guard = None;
        record
            .plugin_state
            .set_lifecycle(instance_id, PluginLifecycleState::Stopped);
        Ok(())
    }
}

#[async_trait]
pub trait BotManager {
    async fn register_bot(&self, definition: BotDefinition) -> Result<()>;
    async fn remove_bot(&self, bot_id: &str) -> Result<()>;
    async fn start_bot(&self, bot_id: &str) -> Result<()>;
    async fn stop_bot(&self, bot_id: &str) -> Result<()>;
    async fn restart_bot(&self, bot_id: &str) -> Result<()>;
    async fn bot_status(&self, bot_id: &str) -> Result<BotStatus>;
}

#[async_trait]
pub trait PluginManagerApi {
    async fn list_plugins(&self, bot_id: &str) -> Result<Vec<(String, PluginInstanceState)>>;
    async fn enable_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()>;
    async fn disable_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()>;
    async fn reload_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()>;
}

#[async_trait]
pub trait ConfigManager {
    async fn read_config(
        &self,
        bot_id: &str,
        instance_id: &str,
    ) -> Result<Option<PluginConfigSnapshot>>;
    async fn write_config(
        &self,
        bot_id: &str,
        instance_id: &str,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64>;
    async fn apply_config(&self, bot_id: &str, instance_id: &str) -> Result<()>;
}

#[async_trait]
impl BotManager for Supervisor {
    async fn register_bot(&self, definition: BotDefinition) -> Result<()> {
        let bot_id = definition.bot_id.to_string();
        if self.bots.contains_key(&bot_id) {
            return Err(anyhow!("bot `{}` already exists", bot_id));
        }

        let runtime = RuntimeController::new(RuntimeState::Stopped);
        let plugin_state = PluginRuntimeState::default();
        let services = RuntimeServices {
            bot_id: definition.bot_id.clone(),
            platform: definition.platform.clone(),
            scheduler: self.scheduler.clone(),
            store: self.store.clone(),
            sessions: self.sessions.clone(),
            metrics: self.metrics.clone(),
            outbound: self.outbound.clone(),
        };

        let plugins = DashMap::new();
        for spec in &definition.plugins {
            plugin_state.set_enabled(&spec.instance_id, spec.enabled);
            plugins.insert(
                spec.instance_id.clone(),
                Arc::new(PluginSlot {
                    spec: spec.clone(),
                    plugin: Mutex::new(None),
                }),
            );
        }

        self.bots.insert(
            bot_id,
            Arc::new(BotRecord {
                definition,
                runtime,
                plugins,
                plugin_state,
                services,
            }),
        );

        Ok(())
    }

    async fn remove_bot(&self, bot_id: &str) -> Result<()> {
        let Some((_, record)) = self.bots.remove(bot_id) else {
            return Err(anyhow!("bot `{}` is not registered", bot_id));
        };

        let plugin_ids: Vec<_> = record
            .plugins
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for plugin_id in plugin_ids {
            self.stop_plugin_instance(&record, &plugin_id).await?;
        }

        Ok(())
    }

    async fn start_bot(&self, bot_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        record.runtime.start().await?;

        let plugin_ids: Vec<_> = record
            .plugins
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for plugin_id in plugin_ids {
            if let Err(err) = self.ensure_plugin_running(&record, &plugin_id).await {
                record
                    .plugin_state
                    .record_error(&plugin_id, err.to_string());
                record.runtime.fail(err.to_string()).await;
                return Err(err);
            }
        }

        Ok(())
    }

    async fn stop_bot(&self, bot_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        record.runtime.mark_stopping().await?;

        let plugin_ids: Vec<_> = record
            .plugins
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for plugin_id in plugin_ids {
            self.stop_plugin_instance(&record, &plugin_id).await?;
        }

        record.runtime.stop().await?;
        Ok(())
    }

    async fn restart_bot(&self, bot_id: &str) -> Result<()> {
        self.stop_bot(bot_id).await?;
        self.start_bot(bot_id).await
    }

    async fn bot_status(&self, bot_id: &str) -> Result<BotStatus> {
        let record = self.bot_record(bot_id)?;
        Ok(BotStatus {
            bot_id: record.definition.bot_id.clone(),
            platform: record.definition.platform.clone(),
            adapter: record.definition.adapter.clone(),
            runtime: record.runtime.status().await,
        })
    }
}

#[async_trait]
impl PluginManagerApi for Supervisor {
    async fn list_plugins(&self, bot_id: &str) -> Result<Vec<(String, PluginInstanceState)>> {
        let record = self.bot_record(bot_id)?;
        Ok(record.plugin_state.snapshots())
    }

    async fn enable_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        record.plugin_state.set_enabled(instance_id, true);

        if record.runtime.state().await == RuntimeState::Running {
            self.ensure_plugin_running(&record, instance_id).await?;
        }

        Ok(())
    }

    async fn disable_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        record.plugin_state.set_enabled(instance_id, false);
        self.stop_plugin_instance(&record, instance_id).await
    }

    async fn reload_plugin(&self, bot_id: &str, instance_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        self.stop_plugin_instance(&record, instance_id).await?;
        let ensure_running = record.runtime.state().await == RuntimeState::Running;

        if let Some(config) = self.read_config(bot_id, instance_id).await? {
            self.prepare_plugin_instance(&record, instance_id, Some(config), ensure_running)
                .await?;
        } else if ensure_running {
            self.prepare_plugin_instance(&record, instance_id, None, true)
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl ConfigManager for Supervisor {
    async fn read_config(
        &self,
        bot_id: &str,
        instance_id: &str,
    ) -> Result<Option<PluginConfigSnapshot>> {
        Ok(self
            .config_store
            .get(bot_id, instance_id)
            .await?
            .map(PluginConfigSnapshot::from))
    }

    async fn write_config(
        &self,
        bot_id: &str,
        instance_id: &str,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        let version = self
            .config_store
            .put(bot_id, instance_id, content, expected_version)
            .await?;
        let record = self.bot_record(bot_id)?;
        record
            .plugin_state
            .set_desired_config_version(instance_id, version);
        Ok(version)
    }

    async fn apply_config(&self, bot_id: &str, instance_id: &str) -> Result<()> {
        let record = self.bot_record(bot_id)?;
        let config = self
            .read_config(bot_id, instance_id)
            .await?
            .ok_or_else(|| anyhow!("plugin `{}` does not have persisted config", instance_id))?;
        self.prepare_plugin_instance(&record, instance_id, Some(config), false)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config_store::TomlConfigStore;
    use crate::core::supervisor::PluginFactory;

    struct TestFactory;

    impl PluginFactory for TestFactory {
        fn kind(&self) -> &'static str {
            "test"
        }

        fn create(&self, _instance_id: &str) -> Box<dyn ManagedPlugin> {
            Box::new(TestPlugin)
        }
    }

    struct TestPlugin;

    #[async_trait]
    impl ManagedPlugin for TestPlugin {
        fn kind(&self) -> &'static str {
            "test"
        }
    }

    #[tokio::test]
    async fn supervisor_tracks_desired_and_applied_config_versions() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn ConfigStore> = Arc::new(TomlConfigStore::new(dir.path()));
        let catalog = PluginCatalog::new();
        catalog.register(TestFactory);

        let supervisor = Supervisor::new(store).with_catalog(catalog);
        supervisor
            .register_bot(
                BotDefinition::new("bot-a", "console", "console")
                    .with_plugin(PluginInstanceSpec::new("echo", "test")),
            )
            .await
            .unwrap();

        let version = supervisor
            .write_config("bot-a", "echo", "value='v1'", None)
            .await
            .unwrap();
        let listed = supervisor.list_plugins("bot-a").await.unwrap();
        assert_eq!(listed[0].1.desired_config_version, version);
        assert_eq!(listed[0].1.applied_config_version, 0);

        supervisor.apply_config("bot-a", "echo").await.unwrap();
        let listed = supervisor.list_plugins("bot-a").await.unwrap();
        assert_eq!(listed[0].1.applied_config_version, version);
    }

    #[tokio::test]
    async fn supervisor_starts_registered_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn ConfigStore> = Arc::new(TomlConfigStore::new(dir.path()));
        let catalog = PluginCatalog::new();
        catalog.register(TestFactory);

        let supervisor = Supervisor::new(store).with_catalog(catalog);
        supervisor
            .register_bot(
                BotDefinition::new("bot-a", "console", "console")
                    .with_plugin(PluginInstanceSpec::new("echo", "test")),
            )
            .await
            .unwrap();

        supervisor.start_bot("bot-a").await.unwrap();
        let status = supervisor.bot_status("bot-a").await.unwrap();
        assert_eq!(status.runtime.state, RuntimeState::Running);

        let plugins = supervisor.list_plugins("bot-a").await.unwrap();
        assert_eq!(plugins[0].1.lifecycle_state, PluginLifecycleState::Running);
    }
}
