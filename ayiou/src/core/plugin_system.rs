use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::{
    context::Context,
    model::{BotId, CommandInvocation, PlatformId},
    observability::MetricsSink,
    plugin::{DispatchOptions, PluginMetadata, parse_command_line},
    plugin_host::PluginHost,
    plugin_runtime::{PluginLifecycleState, PluginRuntimeState},
    session::SessionStore,
    supervisor::{PluginHealth, RuntimeServices as SupervisorRuntimeServices},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePluginManifest {
    pub kind: String,
    pub description: String,
    pub version: String,
    pub required_capabilities: Vec<Capability>,
    pub optional_capabilities: Vec<Capability>,
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

    pub fn require_capability(mut self, capability: Capability) -> Self {
        self.required_capabilities.push(capability);
        self
    }

    pub fn optional_capability(mut self, capability: Capability) -> Self {
        self.optional_capabilities.push(capability);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    ProactiveSend,
    MessageDelete,
    Reaction,
    GroupModeration,
    RichSegments,
    Custom(String),
}

impl Capability {
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityNegotiation {
    Ready,
    Degraded { missing_optional: Vec<Capability> },
    Failed { missing_required: Vec<Capability> },
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
    pub regex_patterns: Vec<String>,
    pub permissions: Vec<Permission>,
    pub priority: i32,
    pub block: bool,
    pub wildcard: bool,
    pub concurrency: ConcurrencyPolicy,
    pub session: SessionPolicy,
}

impl HandlerDecl {
    pub fn wildcard_message() -> Self {
        Self {
            event_kind: HandlerEventKind::Message,
            commands: Vec::new(),
            command_prefixes: Vec::new(),
            regex_patterns: Vec::new(),
            permissions: Vec::new(),
            priority: 0,
            block: false,
            wildcard: true,
            concurrency: ConcurrencyPolicy::Parallel,
            session: SessionPolicy::None,
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
            regex_patterns: Vec::new(),
            permissions: Vec::new(),
            priority: 0,
            block: false,
            wildcard: false,
            concurrency: ConcurrencyPolicy::Parallel,
            session: SessionPolicy::None,
        }
    }

    pub fn message_regex(patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            event_kind: HandlerEventKind::Message,
            commands: Vec::new(),
            command_prefixes: Vec::new(),
            regex_patterns: patterns.into_iter().map(Into::into).collect(),
            permissions: Vec::new(),
            priority: 0,
            block: false,
            wildcard: false,
            concurrency: ConcurrencyPolicy::Parallel,
            session: SessionPolicy::None,
        }
    }

    pub fn require_permission(mut self, permission: Permission) -> Self {
        self.permissions.push(permission);
        self
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn block(mut self, block: bool) -> Self {
        self.block = block;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    Any,
    User(String),
    Group(String),
    Bot(String),
    PlatformCapability(Capability),
}

impl Permission {
    pub fn user(user_id: impl Into<String>) -> Self {
        Self::User(user_id.into())
    }

    pub fn group(group_id: impl Into<String>) -> Self {
        Self::Group(group_id.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConcurrencyPolicy {
    #[default]
    Parallel,
    Serialize,
    Drop,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SessionPolicy {
    #[default]
    None,
    User,
    Channel,
    Group,
    PluginInstance,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigUpdate {
    pub version: u64,
    pub content: String,
    pub dry_run: bool,
}

impl ConfigUpdate {
    pub fn new(version: u64, content: impl Into<String>) -> Self {
        Self {
            version,
            content: content.into(),
            dry_run: false,
        }
    }

    pub fn dry_run(version: u64, content: impl Into<String>) -> Self {
        Self {
            version,
            content: content.into(),
            dry_run: true,
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

    pub fn provided_capabilities(&self) -> Vec<Capability> {
        let mut capabilities = Vec::new();
        if self.host.sender().is_some() {
            capabilities.push(Capability::ProactiveSend);
        }
        capabilities
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
pub trait RuntimePlugin<C: Sync + 'static>: Send + Sync + 'static {
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

    async fn start(&mut self, _services: RuntimePluginServices<C>) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, _update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        Ok(ApplyConfigOutcome::skipped())
    }

    async fn handle(&self, ctx: &C) -> Result<HandleOutcome>;

    async fn handle_with_invocation(
        &self,
        ctx: &C,
        invocation: Option<CommandInvocation>,
    ) -> Result<HandleOutcome> {
        let _ = invocation;
        self.handle(ctx).await
    }

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}

pub trait RuntimePluginFactory<C: Sync + 'static>: Send + Sync + 'static {
    fn kind(&self) -> &'static str;

    fn create(&self, instance_id: &str) -> Result<Box<dyn RuntimePlugin<C>>>;
}

pub fn negotiate_capabilities(
    manifest: &RuntimePluginManifest,
    provided: &[Capability],
) -> CapabilityNegotiation {
    let missing_required: Vec<_> = manifest
        .required_capabilities
        .iter()
        .filter(|capability| !provided.contains(*capability))
        .cloned()
        .collect();
    if !missing_required.is_empty() {
        return CapabilityNegotiation::Failed { missing_required };
    }

    let missing_optional: Vec<_> = manifest
        .optional_capabilities
        .iter()
        .filter(|capability| !provided.contains(*capability))
        .cloned()
        .collect();
    if missing_optional.is_empty() {
        CapabilityNegotiation::Ready
    } else {
        CapabilityNegotiation::Degraded { missing_optional }
    }
}

pub struct RuntimePluginEngine<C> {
    services: RuntimePluginServices<C>,
    runtime_state: PluginRuntimeState,
    dispatch_options: DispatchOptions,
    plugins: Vec<Box<dyn RuntimePlugin<C>>>,
}

impl<C> RuntimePluginEngine<C>
where
    C: Send + Sync + 'static,
{
    pub fn new(services: RuntimePluginServices<C>, runtime_state: PluginRuntimeState) -> Self {
        Self::with_options(services, runtime_state, DispatchOptions::default())
    }

    pub fn with_options(
        services: RuntimePluginServices<C>,
        runtime_state: PluginRuntimeState,
        dispatch_options: DispatchOptions,
    ) -> Self {
        Self {
            services,
            runtime_state,
            dispatch_options,
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
        let provided_capabilities = self.services.provided_capabilities();
        for plugin in &mut self.plugins {
            let instance_id = plugin.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Initializing);

            if let CapabilityNegotiation::Failed { missing_required } =
                negotiate_capabilities(&plugin.manifest(), &provided_capabilities)
            {
                let err = anyhow!(
                    "plugin `{}` missing required capabilities: {:?}",
                    instance_id,
                    missing_required
                );
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

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

            if let Err(err) = plugin.start(self.services.clone()).await {
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
        if update.dry_run {
            self.runtime_state
                .mark_config_validated(instance_id, update.version);
            self.runtime_state.clear_error(instance_id);
            return Ok(ApplyConfigOutcome::skipped());
        }
        let outcome = plugin.apply_config(update).await?;

        if let Some(version) = outcome.applied_version {
            self.runtime_state.mark_config_applied(instance_id, version);
        }

        self.runtime_state.clear_error(instance_id);
        Ok(outcome)
    }

    pub async fn handle_all(&self, ctx: &C) -> Result<bool>
    where
        C: crate::core::adapter::MsgContext,
    {
        let mut matched = self.matched_handlers(ctx);
        matched.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(left.match_rank.cmp(&right.match_rank))
                .then(left.plugin_index.cmp(&right.plugin_index))
        });

        for candidate in matched {
            let plugin = &self.plugins[candidate.plugin_index];
            match plugin
                .handle_with_invocation(ctx, candidate.invocation.clone())
                .await
            {
                Ok(outcome) => {
                    self.runtime_state.clear_error(plugin.instance_id());
                    if outcome.block || candidate.block {
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

    fn matched_handlers(&self, ctx: &C) -> Vec<MatchedHandler>
    where
        C: crate::core::adapter::MsgContext,
    {
        let text = ctx.text();
        let global_prefixes = self.dispatch_options.command_prefixes();
        let mut matched = Vec::new();

        for (plugin_index, plugin) in self.plugins.iter().enumerate() {
            if !self.runtime_state.is_enabled(plugin.instance_id()) {
                continue;
            }

            let best = plugin
                .declared_handlers()
                .into_iter()
                .filter_map(|handler| {
                    self.match_handler(&text, plugin_index, handler, global_prefixes)
                })
                .min_by(|left, right| {
                    left.priority
                        .cmp(&right.priority)
                        .then(left.match_rank.cmp(&right.match_rank))
                });

            if let Some(best) = best {
                matched.push(best);
            }
        }

        matched
    }

    fn match_handler(
        &self,
        text: &str,
        plugin_index: usize,
        handler: HandlerDecl,
        global_prefixes: &[String],
    ) -> Option<MatchedHandler> {
        if !permissions_match(&handler.permissions, text, &self.services) {
            return None;
        }

        let command_match = if handler.commands.is_empty() {
            None
        } else {
            match_handler_command(
                text,
                &handler.commands,
                &handler.command_prefixes,
                global_prefixes,
            )
        };

        if let Some(invocation) = command_match {
            return Some(MatchedHandler {
                plugin_index,
                priority: handler.priority,
                block: handler.block,
                invocation: Some(invocation),
                match_rank: 0,
            });
        }

        if handler.wildcard {
            return Some(MatchedHandler {
                plugin_index,
                priority: handler.priority,
                block: handler.block,
                invocation: None,
                match_rank: 1,
            });
        }

        if regex_patterns_match(text, &handler.regex_patterns) {
            return Some(MatchedHandler {
                plugin_index,
                priority: handler.priority,
                block: handler.block,
                invocation: None,
                match_rank: 2,
            });
        }

        None
    }
}

#[derive(Clone)]
struct MatchedHandler {
    plugin_index: usize,
    priority: i32,
    block: bool,
    invocation: Option<CommandInvocation>,
    match_rank: u8,
}

fn match_handler_command(
    text: &str,
    commands: &[String],
    handler_prefixes: &[String],
    global_prefixes: &[String],
) -> Option<CommandInvocation> {
    let handler_prefix_refs: Vec<&str> = handler_prefixes.iter().map(String::as_str).collect();
    let global_prefix_refs: Vec<&str> = global_prefixes.iter().map(String::as_str).collect();

    parse_command_line(text, &handler_prefix_refs)
        .filter(|invocation| {
            commands
                .iter()
                .any(|command| command == invocation.command())
        })
        .or_else(|| {
            parse_command_line(text, &global_prefix_refs).filter(|invocation| {
                commands
                    .iter()
                    .any(|command| command == invocation.command())
            })
        })
        .or_else(|| {
            parse_command_line(text, &[]).filter(|invocation| {
                commands
                    .iter()
                    .any(|command| command == invocation.command())
            })
        })
}

fn regex_patterns_match(text: &str, patterns: &[String]) -> bool {
    !patterns.is_empty()
        && patterns
            .iter()
            .any(|pattern| regex::Regex::new(pattern).is_ok_and(|regex| regex.is_match(text)))
}

fn permissions_match<C>(
    permissions: &[Permission],
    text: &str,
    services: &RuntimePluginServices<C>,
) -> bool {
    permissions.iter().all(|permission| match permission {
        Permission::Any => true,
        Permission::User(user_id) => {
            services
                .bot_id
                .as_ref()
                .is_some_and(|bot_id| bot_id.as_str() == user_id)
                || text.contains(user_id)
        }
        Permission::Group(group_id) => text.contains(group_id),
        Permission::Bot(bot_id) => services
            .bot_id
            .as_ref()
            .is_some_and(|current| current.as_str() == bot_id),
        Permission::PlatformCapability(capability) => {
            services.provided_capabilities().contains(capability)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use crate::core::{
        adapter::MsgContext,
        plugin::PluginMetadata,
        plugin_host::PluginHost,
        scheduler::{Scheduler, TokioScheduler},
        storage::{MemoryStore, Store},
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

    struct PriorityPlugin {
        instance_id: &'static str,
        priority: i32,
        block_decl: bool,
        handler: HandlerDecl,
        manifest: RuntimePluginManifest,
        hits: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl RuntimePlugin<TestCtx> for PriorityPlugin {
        fn instance_id(&self) -> &str {
            self.instance_id
        }

        fn kind(&self) -> &str {
            self.instance_id
        }

        fn meta(&self) -> PluginMetadata {
            PluginMetadata::new(self.instance_id)
        }

        fn manifest(&self) -> RuntimePluginManifest {
            self.manifest.clone()
        }

        fn declared_handlers(&self) -> Vec<HandlerDecl> {
            let mut handler = self.handler.clone();
            handler.priority = self.priority;
            handler.block = self.block_decl;
            vec![handler]
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
            self.hits.lock().unwrap().push(self.instance_id);
            Ok(HandleOutcome::pass())
        }
    }

    #[tokio::test]
    async fn runtime_plugin_engine_orders_handlers_by_priority_and_respects_block() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push(Box::new(PriorityPlugin {
            instance_id: "late",
            priority: 20,
            block_decl: false,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("late"),
            hits: hits.clone(),
        }));
        engine.push(Box::new(PriorityPlugin {
            instance_id: "first",
            priority: 5,
            block_decl: true,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("first"),
            hits: hits.clone(),
        }));
        engine.push(Box::new(PriorityPlugin {
            instance_id: "never",
            priority: 30,
            block_decl: false,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("never"),
            hits: hits.clone(),
        }));
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        let blocked = engine.handle_all(&TestCtx::default()).await.unwrap();

        assert!(blocked);
        assert_eq!(*hits.lock().unwrap(), vec!["first"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_filters_by_regex_and_permission() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push(Box::new(PriorityPlugin {
            instance_id: "regex",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::message_regex(["^hello\\s+\\w+$"]),
            manifest: RuntimePluginManifest::new("regex"),
            hits: hits.clone(),
        }));
        engine.push(Box::new(PriorityPlugin {
            instance_id: "admin-only",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message().require_permission(Permission::user("admin")),
            manifest: RuntimePluginManifest::new("admin-only"),
            hits: hits.clone(),
        }));
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        engine
            .handle_all(&TestCtx {
                text: "hello world".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["regex"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_supports_dry_run_config_lifecycle() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(PriorityPlugin {
            instance_id: "configurable",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("configurable"),
            hits,
        }));

        let outcome = engine
            .apply_config("configurable", ConfigUpdate::dry_run(7, "enabled=true"))
            .await
            .unwrap();

        let snapshot = state.snapshot("configurable");
        assert_eq!(outcome, ApplyConfigOutcome::skipped());
        assert_eq!(snapshot.desired_config_version, 7);
        assert_eq!(snapshot.applied_config_version, 0);
        assert_eq!(
            snapshot.config_lifecycle_state,
            crate::core::plugin_runtime::ConfigLifecycleState::Validated
        );
    }

    #[tokio::test]
    async fn runtime_plugin_engine_fails_startup_when_required_capability_is_missing() {
        let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let host = PluginHost::new(scheduler, store, None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(PriorityPlugin {
            instance_id: "sender",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("sender")
                .require_capability(Capability::ProactiveSend),
            hits,
        }));

        let err = engine.init_all().await.unwrap_err();
        let snapshot = state.snapshot("sender");
        assert!(err.to_string().contains("missing required capabilities"));
        assert_eq!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
    }
}
