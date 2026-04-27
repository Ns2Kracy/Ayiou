use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::{
    context::Context,
    model::{BotId, PlatformId},
    observability::MetricsSink,
    plugin::{Plugin, PluginMetadata},
    plugin_host::PluginHost,
    plugin_runtime::{PluginLifecycleState, PluginRuntimeState},
    session::SessionStore,
    supervisor::{
        ManagedPlugin, PluginConfigSnapshot, PluginHealth,
        RuntimeServices as SupervisorRuntimeServices,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePluginManifest {
    pub kind: String,
    pub description: String,
    pub version: String,
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
}

impl RuntimePluginManifest {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
            required_capabilities: Vec::new(),
            optional_capabilities: Vec::new(),
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HandlerEventKind {
    Any,
    #[default]
    Message,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerDecl {
    pub event_kind: HandlerEventKind,
    pub commands: Vec<String>,
    pub command_prefixes: Vec<String>,
    pub priority: i32,
    pub block: bool,
    pub wildcard: bool,
}

impl HandlerDecl {
    pub fn wildcard_message() -> Self {
        Self {
            event_kind: HandlerEventKind::Message,
            commands: Vec::new(),
            command_prefixes: Vec::new(),
            priority: 0,
            block: false,
            wildcard: true,
        }
    }

    pub fn message_commands(
        commands: impl IntoIterator<Item = impl Into<String>>,
        command_prefixes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            event_kind: HandlerEventKind::Message,
            commands: commands.into_iter().map(Into::into).collect(),
            command_prefixes: command_prefixes.into_iter().map(Into::into).collect(),
            priority: 0,
            block: false,
            wildcard: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigUpdate {
    pub version: u64,
    pub content: String,
}

impl ConfigUpdate {
    pub fn new(version: u64, content: impl Into<String>) -> Self {
        Self {
            version,
            content: content.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ApplyConfigOutcome {
    pub applied_version: Option<u64>,
}

impl ApplyConfigOutcome {
    pub fn applied(version: u64) -> Self {
        Self {
            applied_version: Some(version),
        }
    }

    pub fn skipped() -> Self {
        Self {
            applied_version: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HandleOutcome {
    pub block: bool,
}

impl HandleOutcome {
    pub fn pass() -> Self {
        Self { block: false }
    }

    pub fn block() -> Self {
        Self { block: true }
    }

    pub fn from_block(block: bool) -> Self {
        Self { block }
    }
}

pub struct RuntimePluginServices<C> {
    pub host: PluginHost<C>,
    pub bot_id: Option<BotId>,
    pub platform: Option<PlatformId>,
    pub sessions: Option<Arc<dyn SessionStore>>,
    pub metrics: Option<Arc<dyn MetricsSink>>,
}

impl<C> Clone for RuntimePluginServices<C> {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            bot_id: self.bot_id.clone(),
            platform: self.platform.clone(),
            sessions: self.sessions.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl<C> RuntimePluginServices<C> {
    pub fn new(host: PluginHost<C>) -> Self {
        Self {
            host,
            bot_id: None,
            platform: None,
            sessions: None,
            metrics: None,
        }
    }

    pub fn with_identity(
        mut self,
        bot_id: impl Into<BotId>,
        platform: impl Into<PlatformId>,
    ) -> Self {
        self.bot_id = Some(bot_id.into());
        self.platform = Some(platform.into());
        self
    }

    pub fn with_sessions(mut self, sessions: Arc<dyn SessionStore>) -> Self {
        self.sessions = Some(sessions);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsSink>) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

impl RuntimePluginServices<Context> {
    pub fn try_as_supervisor_runtime_services(&self) -> Result<SupervisorRuntimeServices> {
        let bot_id = self
            .bot_id
            .clone()
            .ok_or_else(|| anyhow!("missing bot id in runtime plugin services"))?;
        let platform = self
            .platform
            .clone()
            .ok_or_else(|| anyhow!("missing platform in runtime plugin services"))?;
        let sessions = self
            .sessions
            .clone()
            .ok_or_else(|| anyhow!("missing session store in runtime plugin services"))?;
        let metrics = self
            .metrics
            .clone()
            .ok_or_else(|| anyhow!("missing metrics sink in runtime plugin services"))?;

        Ok(SupervisorRuntimeServices {
            bot_id,
            platform,
            scheduler: self.host.scheduler(),
            store: self.host.store(),
            sessions,
            metrics,
            outbound: self.host.sender(),
        })
    }
}

#[async_trait]
pub trait RuntimePlugin<C: 'static>: Send + Sync + 'static {
    fn instance_id(&self) -> &str;

    fn kind(&self) -> &str;

    fn meta(&self) -> PluginMetadata;

    fn manifest(&self) -> RuntimePluginManifest {
        let meta = self.meta();
        RuntimePluginManifest::new(self.kind())
            .description(meta.description)
            .version(meta.version)
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        Vec::new()
    }

    async fn init(&mut self, _services: RuntimePluginServices<C>) -> Result<()> {
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, _update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        Ok(ApplyConfigOutcome::skipped())
    }

    async fn handle(&self, ctx: &C) -> Result<HandleOutcome>;

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}

pub trait RuntimePluginFactory<C: 'static>: Send + Sync + 'static {
    fn kind(&self) -> &'static str;

    fn create(&self, instance_id: &str) -> Result<Box<dyn RuntimePlugin<C>>>;
}

pub struct RuntimePluginEngine<C> {
    services: RuntimePluginServices<C>,
    runtime_state: PluginRuntimeState,
    plugins: Vec<Box<dyn RuntimePlugin<C>>>,
}

impl<C> RuntimePluginEngine<C>
where
    C: Send + Sync + 'static,
{
    pub fn new(services: RuntimePluginServices<C>, runtime_state: PluginRuntimeState) -> Self {
        Self {
            services,
            runtime_state,
            plugins: Vec::new(),
        }
    }

    pub fn push(&mut self, plugin: Box<dyn RuntimePlugin<C>>) {
        self.runtime_state
            .set_lifecycle(plugin.instance_id(), PluginLifecycleState::Registered);
        self.plugins.push(plugin);
    }

    pub fn plugins(&self) -> &[Box<dyn RuntimePlugin<C>>] {
        &self.plugins
    }

    pub async fn init_all(&mut self) -> Result<()> {
        for plugin in &mut self.plugins {
            let instance_id = plugin.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Initializing);

            if let Err(err) = plugin.init(self.services.clone()).await {
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

            self.runtime_state.clear_error(&instance_id);
        }

        Ok(())
    }

    pub async fn start_all(&mut self) -> Result<()> {
        for plugin in &mut self.plugins {
            let instance_id = plugin.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Starting);

            if let Err(err) = plugin.start().await {
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Running);
            self.runtime_state.clear_error(&instance_id);
        }

        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        let mut first_error = None;

        for plugin in self.plugins.iter_mut().rev() {
            let instance_id = plugin.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Stopping);

            if let Err(err) = plugin.stop().await {
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                if first_error.is_none() {
                    first_error = Some(err);
                }
            } else {
                self.runtime_state
                    .set_lifecycle(&instance_id, PluginLifecycleState::Stopped);
                self.runtime_state.clear_error(&instance_id);
            }
        }

        if let Some(err) = first_error {
            return Err(err);
        }

        Ok(())
    }

    pub async fn apply_config(
        &mut self,
        instance_id: &str,
        update: ConfigUpdate,
    ) -> Result<ApplyConfigOutcome> {
        let plugin = self
            .plugins
            .iter_mut()
            .find(|plugin| plugin.instance_id() == instance_id)
            .ok_or_else(|| anyhow!("plugin instance `{}` is not registered", instance_id))?;

        self.runtime_state
            .set_desired_config_version(instance_id, update.version);
        let outcome = plugin.apply_config(update).await?;

        if let Some(version) = outcome.applied_version {
            self.runtime_state.mark_config_applied(instance_id, version);
        }

        self.runtime_state.clear_error(instance_id);
        Ok(outcome)
    }

    pub async fn handle_all(&self, ctx: &C) -> Result<bool> {
        for plugin in &self.plugins {
            if !self.runtime_state.is_enabled(plugin.instance_id()) {
                continue;
            }

            match plugin.handle(ctx).await {
                Ok(outcome) => {
                    self.runtime_state.clear_error(plugin.instance_id());
                    if outcome.block {
                        return Ok(true);
                    }
                }
                Err(err) => {
                    self.runtime_state
                        .record_error(plugin.instance_id(), err.to_string());
                    return Err(err);
                }
            }
        }

        Ok(false)
    }
}

pub struct LegacyMessagePluginAdapter<C> {
    instance_id: String,
    kind: String,
    plugin: Arc<dyn Plugin<C>>,
    services: Option<RuntimePluginServices<C>>,
}

impl<C> LegacyMessagePluginAdapter<C>
where
    C: 'static,
{
    pub fn new(instance_id: impl Into<String>, plugin: Arc<dyn Plugin<C>>) -> Self {
        let kind = plugin.meta().name.clone();
        Self {
            instance_id: instance_id.into(),
            kind,
            plugin,
            services: None,
        }
    }
}

#[async_trait]
impl<C> RuntimePlugin<C> for LegacyMessagePluginAdapter<C>
where
    C: Send + Sync + 'static,
{
    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn meta(&self) -> PluginMetadata {
        self.plugin.meta()
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        let commands = self.plugin.commands();
        if commands.is_empty() {
            vec![HandlerDecl::wildcard_message()]
        } else {
            vec![HandlerDecl::message_commands(
                commands,
                self.plugin.command_prefixes(),
            )]
        }
    }

    async fn init(&mut self, services: RuntimePluginServices<C>) -> Result<()> {
        self.services = Some(services);
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        let services = self
            .services
            .clone()
            .ok_or_else(|| anyhow!("plugin `{}` has not been initialized", self.instance_id))?;
        self.plugin.start(services.host).await
    }

    async fn handle(&self, ctx: &C) -> Result<HandleOutcome> {
        Ok(HandleOutcome::from_block(self.plugin.handle(ctx).await?))
    }
}

pub struct LegacyManagedPluginAdapter {
    instance_id: String,
    kind: String,
    plugin: Box<dyn ManagedPlugin>,
}

impl LegacyManagedPluginAdapter {
    pub fn new(
        instance_id: impl Into<String>,
        kind: impl Into<String>,
        plugin: Box<dyn ManagedPlugin>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            kind: kind.into(),
            plugin,
        }
    }
}

#[async_trait]
impl RuntimePlugin<Context> for LegacyManagedPluginAdapter {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new(self.instance_id.clone())
            .description(format!("managed plugin kind `{}`", self.kind))
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl {
            event_kind: HandlerEventKind::Any,
            commands: Vec::new(),
            command_prefixes: Vec::new(),
            priority: 0,
            block: false,
            wildcard: true,
        }]
    }

    async fn init(&mut self, services: RuntimePluginServices<Context>) -> Result<()> {
        let runtime_services = services.try_as_supervisor_runtime_services()?;
        self.plugin.init(runtime_services).await
    }

    async fn start(&mut self) -> Result<()> {
        self.plugin.start().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.plugin.stop().await
    }

    async fn apply_config(&mut self, update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        self.plugin
            .apply_config(PluginConfigSnapshot {
                version: update.version,
                content: update.content,
            })
            .await?;
        Ok(ApplyConfigOutcome::applied(update.version))
    }

    async fn handle(&self, ctx: &Context) -> Result<HandleOutcome> {
        self.plugin.handle_event(ctx.event()).await?;
        Ok(HandleOutcome::pass())
    }

    fn health(&self) -> PluginHealth {
        self.plugin.health()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use anyhow::Result;

    use crate::core::{
        adapter::MsgContext,
        model::{ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
        observability::NoopMetrics,
        plugin_host::PluginHost,
        scheduler::{Scheduler, TokioScheduler},
        storage::{MemoryStore, Store},
        supervisor::{ManagedPlugin, PluginConfigSnapshot},
    };

    use super::*;

    #[derive(Clone, Default)]
    struct TestCtx {
        text: String,
    }

    impl MsgContext for TestCtx {
        fn text(&self) -> String {
            self.text.clone()
        }

        fn user_id(&self) -> String {
            "user".to_string()
        }

        fn group_id(&self) -> Option<String> {
            None
        }
    }

    struct StartHandlePlugin {
        starts: Arc<AtomicUsize>,
        handles: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Plugin<TestCtx> for StartHandlePlugin {
        fn meta(&self) -> PluginMetadata {
            PluginMetadata::new("echo")
        }

        fn commands(&self) -> Vec<String> {
            vec!["echo".to_string()]
        }

        fn command_prefixes(&self) -> Vec<String> {
            vec!["/".to_string()]
        }

        async fn start(&self, _host: PluginHost<TestCtx>) -> Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<bool> {
            self.handles.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    #[tokio::test]
    async fn runtime_plugin_engine_bridges_legacy_message_plugins() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let starts = Arc::new(AtomicUsize::new(0));
        let handles = Arc::new(AtomicUsize::new(0));
        let plugin = Arc::new(StartHandlePlugin {
            starts: starts.clone(),
            handles: handles.clone(),
        }) as Arc<dyn Plugin<TestCtx>>;

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(LegacyMessagePluginAdapter::new("echo", plugin)));
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        let blocked = engine.handle_all(&TestCtx::default()).await.unwrap();
        assert!(blocked);
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(handles.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.snapshot("echo").lifecycle_state,
            PluginLifecycleState::Running
        );
        assert_eq!(
            engine.plugins()[0].declared_handlers(),
            vec![HandlerDecl::message_commands(["echo"], ["/"])]
        );
    }

    struct TestManagedPlugin {
        starts: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
        configs: Arc<AtomicUsize>,
        events: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ManagedPlugin for TestManagedPlugin {
        fn kind(&self) -> &'static str {
            "managed-test"
        }

        async fn start(&mut self) -> Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn handle_event(&self, _event: &crate::core::model::EventEnvelope) -> Result<()> {
            self.events.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn apply_config(&mut self, _config: PluginConfigSnapshot) -> Result<()> {
            self.configs.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn managed_plugin_adapter_bridges_lifecycle_and_config() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host)
            .with_identity("bot-a", "console")
            .with_sessions(Arc::new(crate::core::session::MemorySessionStore::new()))
            .with_metrics(Arc::new(NoopMetrics));
        let starts = Arc::new(AtomicUsize::new(0));
        let stops = Arc::new(AtomicUsize::new(0));
        let configs = Arc::new(AtomicUsize::new(0));
        let events = Arc::new(AtomicUsize::new(0));
        let mut plugin = LegacyManagedPluginAdapter::new(
            "instance-a",
            "managed-test",
            Box::new(TestManagedPlugin {
                starts: starts.clone(),
                stops: stops.clone(),
                configs: configs.clone(),
                events: events.clone(),
            }),
        );

        plugin.init(services).await.unwrap();
        plugin.start().await.unwrap();
        plugin
            .apply_config(ConfigUpdate::new(2, "value='v2'"))
            .await
            .unwrap();

        let platform = PlatformId::new("console");
        let user = UserRef::new(platform.clone(), "u1");
        let channel = ChannelRef::direct(platform.clone(), "u1");
        let message = MessageEvent::new(user, channel, "hello");
        let ctx = Context::new(
            EventEnvelope::new("bot-a", platform).with_message(message),
            None,
            (),
        );

        let outcome = plugin.handle(&ctx).await.unwrap();
        plugin.stop().await.unwrap();

        assert!(!outcome.block);
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(configs.load(Ordering::SeqCst), 1);
        assert_eq!(events.load(Ordering::SeqCst), 1);
    }
}
