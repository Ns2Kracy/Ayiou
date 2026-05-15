use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::{
    command::parse_command_line,
    model::{BotId, CommandInvocation, PlatformId},
    plugin_host::PluginHost,
    plugin_runtime::{PluginInstanceState, PluginLifecycleState, PluginRuntimeState},
    service::{RuntimeService, ServiceDescriptor, ServiceKey, ServiceRegistry},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
}

impl PluginMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }

    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }
}

impl Default for PluginMetadata {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchOptions {
    command_prefixes: Arc<[String]>,
}

impl DispatchOptions {
    pub fn new(command_prefixes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut prefixes: Vec<String> = command_prefixes
            .into_iter()
            .map(Into::into)
            .filter(|p| !p.is_empty())
            .collect();
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        prefixes.dedup();

        Self {
            command_prefixes: prefixes.into(),
        }
    }

    #[must_use]
    pub fn command_prefixes(&self) -> &[String] {
        self.command_prefixes.as_ref()
    }
}

impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            command_prefixes: Arc::from([]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePluginManifest {
    pub kind: String,
    pub description: String,
    pub version: String,
    pub required_capabilities: Vec<Capability>,
    pub optional_capabilities: Vec<Capability>,
    pub required_services: Vec<ServiceKey>,
    pub optional_services: Vec<ServiceKey>,
}

impl RuntimePluginManifest {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
            required_capabilities: Vec::new(),
            optional_capabilities: Vec::new(),
            required_services: Vec::new(),
            optional_services: Vec::new(),
        }
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    #[must_use]
    pub fn require_capability(mut self, capability: Capability) -> Self {
        self.required_capabilities.push(capability);
        self
    }

    #[must_use]
    pub fn optional_capability(mut self, capability: Capability) -> Self {
        self.optional_capabilities.push(capability);
        self
    }

    #[must_use]
    pub fn require_service<S>(mut self) -> Self
    where
        S: RuntimeService,
    {
        self.required_services.push(ServiceKey::of::<S>());
        self
    }

    #[must_use]
    pub fn optional_service<S>(mut self) -> Self
    where
        S: RuntimeService,
    {
        self.optional_services.push(ServiceKey::of::<S>());
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
}

impl HandlerDecl {
    #[must_use]
    pub const fn wildcard_message() -> Self {
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
        }
    }

    #[must_use]
    pub fn require_permission(mut self, permission: Permission) -> Self {
        self.permissions.push(permission);
        self
    }

    #[must_use]
    pub const fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub const fn block(mut self, block: bool) -> Self {
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
    #[must_use]
    pub const fn applied(version: u64) -> Self {
        Self {
            applied_version: Some(version),
        }
    }

    #[must_use]
    pub const fn skipped() -> Self {
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
    #[must_use]
    pub const fn pass() -> Self {
        Self { block: false }
    }

    #[must_use]
    pub const fn block() -> Self {
        Self { block: true }
    }

    #[must_use]
    pub const fn from_block(block: bool) -> Self {
        Self { block }
    }
}

pub struct RuntimePluginServices<C> {
    pub host: PluginHost<C>,
    pub instance_id: Option<String>,
    pub bot_id: Option<BotId>,
    pub platform: Option<PlatformId>,
    pub capabilities: Vec<Capability>,
    pub service_registry: ServiceRegistry,
}

impl<C> Clone for RuntimePluginServices<C> {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            instance_id: self.instance_id.clone(),
            bot_id: self.bot_id.clone(),
            platform: self.platform.clone(),
            capabilities: self.capabilities.clone(),
            service_registry: self.service_registry.clone(),
        }
    }
}

impl<C> RuntimePluginServices<C> {
    #[must_use]
    pub fn new(host: PluginHost<C>) -> Self {
        Self {
            host,
            instance_id: None,
            bot_id: None,
            platform: None,
            capabilities: Vec::new(),
            service_registry: ServiceRegistry::default(),
        }
    }

    #[must_use]
    pub fn with_instance_id(mut self, instance_id: impl Into<String>) -> Self {
        self.instance_id = Some(instance_id.into());
        self
    }

    #[must_use]
    pub fn with_identity(
        mut self,
        bot_id: impl Into<BotId>,
        platform: impl Into<PlatformId>,
    ) -> Self {
        self.bot_id = Some(bot_id.into());
        self.platform = Some(platform.into());
        self
    }

    #[must_use]
    pub fn with_capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_service_registry(mut self, service_registry: ServiceRegistry) -> Self {
        self.service_registry = service_registry;
        self
    }

    #[must_use]
    pub fn service<S>(&self) -> Option<Arc<S>>
    where
        S: RuntimeService,
    {
        self.service_registry.get::<S>()
    }

    pub fn require_service<S>(&self) -> Result<Arc<S>>
    where
        S: RuntimeService,
    {
        self.service_registry.require::<S>()
    }

    #[must_use]
    pub fn service_descriptor<S>(&self) -> Option<ServiceDescriptor>
    where
        S: RuntimeService,
    {
        self.service_registry.descriptor::<S>()
    }

    #[must_use]
    pub fn service_descriptors(&self) -> Vec<ServiceDescriptor> {
        self.service_registry.descriptors()
    }

    #[must_use]
    pub fn provided_capabilities(&self) -> Vec<Capability> {
        let mut capabilities = self.capabilities.clone();
        if self.host.sender().is_some() && !capabilities.contains(&Capability::ProactiveSend) {
            capabilities.push(Capability::ProactiveSend);
        }
        capabilities
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHealth {
    pub healthy: bool,
    pub detail: Option<String>,
}

impl PluginHealth {
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            healthy: true,
            detail: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePluginSnapshot {
    pub instance_id: String,
    pub kind: String,
    pub meta: PluginMetadata,
    pub manifest: RuntimePluginManifest,
    pub lifecycle: PluginInstanceState,
    pub health: PluginHealth,
}

#[async_trait]
pub trait RuntimePlugin<C: Sync + 'static>: Send + Sync + 'static {
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

pub struct RegisteredPlugin<C> {
    instance_id: String,
    plugin: Box<dyn RuntimePlugin<C>>,
}

impl<C> RegisteredPlugin<C>
where
    C: Sync + 'static,
{
    pub fn new(instance_id: impl Into<String>, plugin: Box<dyn RuntimePlugin<C>>) -> Self {
        Self {
            instance_id: instance_id.into(),
            plugin,
        }
    }

    #[must_use]
    pub fn from_plugin(plugin: Box<dyn RuntimePlugin<C>>) -> Self {
        let instance_id = default_instance_id(plugin.as_ref());
        Self::new(instance_id, plugin)
    }

    #[must_use]
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    #[must_use]
    pub fn plugin(&self) -> &dyn RuntimePlugin<C> {
        self.plugin.as_ref()
    }

    pub fn plugin_mut(&mut self) -> &mut dyn RuntimePlugin<C> {
        self.plugin.as_mut()
    }

    #[must_use]
    pub fn into_parts(self) -> (String, Box<dyn RuntimePlugin<C>>) {
        (self.instance_id, self.plugin)
    }
}

fn default_instance_id<C>(plugin: &dyn RuntimePlugin<C>) -> String
where
    C: Sync + 'static,
{
    let meta = plugin.meta();
    if meta.name.trim().is_empty() {
        plugin.kind().to_string()
    } else {
        meta.name
    }
}

#[must_use]
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

fn missing_required_services(
    manifest: &RuntimePluginManifest,
    registry: &ServiceRegistry,
) -> Vec<ServiceKey> {
    manifest
        .required_services
        .iter()
        .filter(|service| !registry.contains_key(service))
        .copied()
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRequirementFailure {
    pub instance_id: String,
    pub missing_capabilities: Vec<Capability>,
    pub missing_services: Vec<ServiceKey>,
}

impl PluginRequirementFailure {
    fn message(&self) -> String {
        let mut parts = Vec::new();
        if !self.missing_capabilities.is_empty() {
            parts.push(format!(
                "missing required capabilities: {:?}",
                self.missing_capabilities
            ));
        }
        if !self.missing_services.is_empty() {
            let missing_names: Vec<_> = self
                .missing_services
                .iter()
                .map(ServiceKey::type_name)
                .collect();
            parts.push(format!("missing required services: {missing_names:?}"));
        }

        format!("plugin `{}` {}", self.instance_id, parts.join("; "))
    }
}

pub struct RuntimePluginEngine<C> {
    services: RuntimePluginServices<C>,
    runtime_state: PluginRuntimeState,
    dispatch_options: DispatchOptions,
    plugins: Vec<RegisteredPlugin<C>>,
}

impl<C> RuntimePluginEngine<C>
where
    C: Send + Sync + 'static,
{
    #[must_use]
    pub fn new(services: RuntimePluginServices<C>, runtime_state: PluginRuntimeState) -> Self {
        Self::with_options(services, runtime_state, DispatchOptions::default())
    }

    #[must_use]
    pub const fn with_options(
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
        self.push_registered(RegisteredPlugin::from_plugin(plugin));
    }

    pub fn push_as(&mut self, instance_id: impl Into<String>, plugin: Box<dyn RuntimePlugin<C>>) {
        self.push_registered(RegisteredPlugin::new(instance_id, plugin));
    }

    fn push_registered(&mut self, plugin: RegisteredPlugin<C>) {
        self.runtime_state
            .set_lifecycle(plugin.instance_id(), PluginLifecycleState::Registered);
        self.plugins.push(plugin);
    }

    #[must_use]
    pub fn plugins(&self) -> &[RegisteredPlugin<C>] {
        &self.plugins
    }

    #[must_use]
    pub fn plugin_snapshots(&self) -> Vec<RuntimePluginSnapshot> {
        let mut snapshots: Vec<_> = self
            .plugins
            .iter()
            .map(|registered| {
                let plugin = registered.plugin();
                let instance_id = registered.instance_id().to_string();
                RuntimePluginSnapshot {
                    lifecycle: self.runtime_state.snapshot(&instance_id),
                    instance_id,
                    kind: plugin.kind().to_string(),
                    meta: plugin.meta(),
                    manifest: plugin.manifest(),
                    health: plugin.health(),
                }
            })
            .collect();
        snapshots.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        snapshots
    }

    pub async fn init_all(&mut self) -> Result<()> {
        let failures = self.validate_startup_requirements();
        if !failures.is_empty() {
            let messages: Vec<_> = failures
                .iter()
                .map(PluginRequirementFailure::message)
                .collect();
            for (failure, message) in failures.iter().zip(&messages) {
                self.runtime_state
                    .record_error(&failure.instance_id, message.clone());
            }
            return Err(anyhow!("{}", messages.join("; ")));
        }

        for registered in &mut self.plugins {
            let instance_id = registered.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Initializing);

            if let Err(err) = registered
                .plugin_mut()
                .init(self.services.clone().with_instance_id(instance_id.clone()))
                .await
            {
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

            self.runtime_state.clear_error(&instance_id);
        }

        Ok(())
    }

    #[must_use]
    pub fn validate_startup_requirements(&self) -> Vec<PluginRequirementFailure> {
        let provided_capabilities = self.services.provided_capabilities();
        self.plugins
            .iter()
            .filter_map(|registered| {
                let manifest = registered.plugin().manifest();
                let missing_capabilities =
                    if let CapabilityNegotiation::Failed { missing_required } =
                        negotiate_capabilities(&manifest, &provided_capabilities)
                    {
                        missing_required
                    } else {
                        Vec::new()
                    };
                let missing_services =
                    missing_required_services(&manifest, &self.services.service_registry);

                (!missing_capabilities.is_empty() || !missing_services.is_empty()).then(|| {
                    PluginRequirementFailure {
                        instance_id: registered.instance_id().to_string(),
                        missing_capabilities,
                        missing_services,
                    }
                })
            })
            .collect()
    }

    pub async fn start_all(&mut self) -> Result<()> {
        for registered in &mut self.plugins {
            let instance_id = registered.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Starting);

            if let Err(err) = registered
                .plugin_mut()
                .start(self.services.clone().with_instance_id(instance_id.clone()))
                .await
            {
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

        for registered in self.plugins.iter_mut().rev() {
            let instance_id = registered.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Stopping);

            if let Err(err) = registered.plugin_mut().stop().await {
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
        let registered = self
            .plugins
            .iter_mut()
            .find(|registered| registered.instance_id() == instance_id)
            .ok_or_else(|| anyhow!("plugin instance `{instance_id}` is not registered"))?;

        self.runtime_state
            .set_desired_config_version(instance_id, update.version);
        if update.dry_run {
            self.runtime_state
                .mark_config_validated(instance_id, update.version);
            self.runtime_state.clear_error(instance_id);
            return Ok(ApplyConfigOutcome::skipped());
        }
        let outcome = registered.plugin_mut().apply_config(update).await?;

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
            let registered = &self.plugins[candidate.plugin_index];
            match registered
                .plugin()
                .handle_with_invocation(ctx, candidate.invocation.clone())
                .await
            {
                Ok(outcome) => {
                    self.runtime_state.clear_error(registered.instance_id());
                    if outcome.block || candidate.block {
                        return Ok(true);
                    }
                }
                Err(err) => {
                    self.runtime_state
                        .record_error(registered.instance_id(), err.to_string());
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

        for (plugin_index, registered) in self.plugins.iter().enumerate() {
            if !self.runtime_state.is_enabled(registered.instance_id()) {
                continue;
            }

            let best = registered
                .plugin()
                .declared_handlers()
                .into_iter()
                .filter_map(|handler| {
                    self.match_handler(ctx, &text, plugin_index, &handler, global_prefixes)
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
        ctx: &C,
        text: &str,
        plugin_index: usize,
        handler: &HandlerDecl,
        global_prefixes: &[String],
    ) -> Option<MatchedHandler>
    where
        C: crate::core::adapter::MsgContext,
    {
        if !permissions_match(&handler.permissions, ctx, &self.services) {
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
    ctx: &C,
    services: &RuntimePluginServices<C>,
) -> bool
where
    C: crate::core::adapter::MsgContext,
{
    permissions.iter().all(|permission| match permission {
        Permission::Any => true,
        Permission::User(user_id) => ctx.user_id() == *user_id,
        Permission::Group(group_id) => ctx.group_id().as_deref() == Some(group_id.as_str()),
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
        plugin_host::PluginHost,
        service::{RuntimeService, ServiceKey, ServiceRegistry},
    };

    use super::*;

    #[derive(Clone, Default)]
    struct TestCtx {
        text: String,
        user_id: String,
        group_id: Option<String>,
    }

    impl MsgContext for TestCtx {
        fn text(&self) -> String {
            self.text.clone()
        }

        fn user_id(&self) -> String {
            if self.user_id.is_empty() {
                "user".to_string()
            } else {
                self.user_id.clone()
            }
        }

        fn group_id(&self) -> Option<String> {
            self.group_id.clone()
        }
    }

    #[derive(Debug)]
    struct TestCounterService {
        value: usize,
    }

    impl RuntimeService for TestCounterService {
        fn name(&self) -> &'static str {
            "test-counter"
        }
    }

    #[test]
    fn runtime_plugin_services_provide_registered_services() {
        let host = PluginHost::<TestCtx>::new(None);
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 7 });
        let services = RuntimePluginServices::new(host).with_service_registry(registry);

        let service = services
            .require_service::<TestCounterService>()
            .expect("service should be available");

        assert_eq!(service.value, 7);
    }

    #[test]
    fn runtime_plugin_services_describe_registered_services() {
        let host = PluginHost::<TestCtx>::new(None);
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 7 });
        let services = RuntimePluginServices::new(host).with_service_registry(registry);

        let descriptor = services
            .service_descriptor::<TestCounterService>()
            .expect("counter service should have a descriptor");

        assert_eq!(descriptor.key, ServiceKey::of::<TestCounterService>());
        assert_eq!(descriptor.name, "test-counter");
        assert_eq!(descriptor.version, "0.1.0");
        assert_eq!(services.service_descriptors(), vec![descriptor]);
    }

    #[test]
    fn runtime_plugin_manifest_records_service_dependencies() {
        let manifest = RuntimePluginManifest::new("service-user")
            .require_service::<TestCounterService>()
            .optional_service::<MissingService>();

        assert_eq!(
            manifest.required_services,
            vec![ServiceKey::of::<TestCounterService>()]
        );
        assert_eq!(
            manifest.optional_services,
            vec![ServiceKey::of::<MissingService>()]
        );
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

    #[derive(Debug)]
    struct MissingService;

    impl RuntimeService for MissingService {
        fn name(&self) -> &'static str {
            "missing"
        }
    }

    struct DependencyPlugin {
        manifest: RuntimePluginManifest,
        init_calls: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin<TestCtx> for DependencyPlugin {
        fn kind(&self) -> &'static str {
            "dependency"
        }

        fn meta(&self) -> PluginMetadata {
            PluginMetadata::new("dependency")
        }

        fn manifest(&self) -> RuntimePluginManifest {
            self.manifest.clone()
        }

        async fn init(&mut self, _services: RuntimePluginServices<TestCtx>) -> Result<()> {
            *self.init_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    struct SnapshotPlugin {
        kind: &'static str,
        meta_name: &'static str,
        manifest: RuntimePluginManifest,
        health: PluginHealth,
        init_calls: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin<TestCtx> for SnapshotPlugin {
        fn kind(&self) -> &'static str {
            self.kind
        }

        fn meta(&self) -> PluginMetadata {
            PluginMetadata::new(self.meta_name)
                .description("snapshot plugin")
                .version("9.9.9")
        }

        fn manifest(&self) -> RuntimePluginManifest {
            self.manifest.clone()
        }

        async fn init(&mut self, _services: RuntimePluginServices<TestCtx>) -> Result<()> {
            *self.init_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }

        fn health(&self) -> PluginHealth {
            self.health.clone()
        }
    }

    #[test]
    fn runtime_plugin_engine_reports_plugin_snapshots() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        state.set_enabled("snapshot.instance", false);
        state.set_desired_config_version("snapshot.instance", 12);
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "snapshot.instance",
            Box::new(SnapshotPlugin {
                kind: "snapshot-kind",
                meta_name: "snapshot-meta",
                manifest: RuntimePluginManifest::new("snapshot-kind")
                    .require_capability(Capability::Reaction)
                    .require_service::<TestCounterService>(),
                health: PluginHealth {
                    healthy: false,
                    detail: Some("degraded".to_string()),
                },
                init_calls,
            }),
        );

        let snapshots = engine.plugin_snapshots();

        assert_eq!(snapshots.len(), 1);
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.instance_id, "snapshot.instance");
        assert_eq!(snapshot.kind, "snapshot-kind");
        assert_eq!(snapshot.meta.name, "snapshot-meta");
        assert_eq!(snapshot.meta.description, "snapshot plugin");
        assert_eq!(snapshot.meta.version, "9.9.9");
        assert_eq!(snapshot.manifest.kind, "snapshot-kind");
        assert_eq!(
            snapshot.manifest.required_capabilities,
            vec![Capability::Reaction]
        );
        assert_eq!(
            snapshot.manifest.required_services,
            vec![ServiceKey::of::<TestCounterService>()]
        );
        assert!(!snapshot.lifecycle.enabled);
        assert_eq!(snapshot.lifecycle.desired_config_version, 12);
        assert_eq!(
            snapshot.lifecycle.config_lifecycle_state,
            crate::core::plugin_runtime::ConfigLifecycleState::Draft
        );
        assert_eq!(
            snapshot.health,
            PluginHealth {
                healthy: false,
                detail: Some("degraded".to_string())
            }
        );
    }

    #[test]
    fn plugin_snapshots_are_sorted_by_instance_id() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();

        let mut engine = RuntimePluginEngine::new(services, state);
        for instance_id in ["zeta", "alpha", "middle"] {
            engine.push_as(
                instance_id,
                Box::new(DependencyPlugin {
                    manifest: RuntimePluginManifest::new(instance_id),
                    init_calls: Arc::new(std::sync::Mutex::new(0)),
                }),
            );
        }

        let instance_ids: Vec<_> = engine
            .plugin_snapshots()
            .into_iter()
            .map(|snapshot| snapshot.instance_id)
            .collect();

        assert_eq!(instance_ids, vec!["alpha", "middle", "zeta"]);
    }

    #[test]
    fn plugin_snapshots_do_not_trigger_lifecycle_hooks() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "snapshot-only",
            Box::new(SnapshotPlugin {
                kind: "snapshot-only",
                meta_name: "snapshot-only",
                manifest: RuntimePluginManifest::new("snapshot-only"),
                health: PluginHealth::healthy(),
                init_calls: init_calls.clone(),
            }),
        );

        let snapshots = engine.plugin_snapshots();

        assert_eq!(snapshots.len(), 1);
        assert_eq!(*init_calls.lock().unwrap(), 0);
        assert_eq!(
            state.snapshot("snapshot-only").lifecycle_state,
            PluginLifecycleState::Registered
        );
    }

    #[tokio::test]
    async fn runtime_plugin_engine_fails_init_when_required_service_is_missing() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(DependencyPlugin {
            manifest: RuntimePluginManifest::new("dependency")
                .require_service::<TestCounterService>(),
            init_calls: init_calls.clone(),
        }));

        let err = engine.init_all().await.unwrap_err();
        let snapshot = state.snapshot("dependency");

        assert!(err.to_string().contains("missing required services"));
        assert!(
            err.to_string()
                .contains(std::any::type_name::<TestCounterService>())
        );
        assert_eq!(*init_calls.lock().unwrap(), 0);
        assert_eq!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert!(
            snapshot
                .last_error
                .as_deref()
                .is_some_and(|err| err.contains("missing required services"))
        );
    }

    #[tokio::test]
    async fn runtime_plugin_engine_initializes_when_required_service_is_available() {
        let host = PluginHost::new(None);
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 1 });
        let services = RuntimePluginServices::new(host).with_service_registry(registry);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(DependencyPlugin {
            manifest: RuntimePluginManifest::new("dependency")
                .require_service::<TestCounterService>(),
            init_calls: init_calls.clone(),
        }));

        engine.init_all().await.unwrap();
        let snapshot = state.snapshot("dependency");

        assert_eq!(*init_calls.lock().unwrap(), 1);
        assert_ne!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert!(snapshot.last_error.is_none());
    }

    #[tokio::test]
    async fn runtime_plugin_engine_allows_missing_optional_services() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(DependencyPlugin {
            manifest: RuntimePluginManifest::new("dependency")
                .optional_service::<TestCounterService>(),
            init_calls: init_calls.clone(),
        }));

        engine.init_all().await.unwrap();
        let snapshot = state.snapshot("dependency");

        assert_eq!(*init_calls.lock().unwrap(), 1);
        assert_ne!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert!(snapshot.last_error.is_none());
    }

    #[tokio::test]
    async fn runtime_plugin_engine_preflights_required_dependencies_before_init() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let ready_init_calls = Arc::new(std::sync::Mutex::new(0));
        let missing_service_init_calls = Arc::new(std::sync::Mutex::new(0));
        let missing_capability_init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "ready",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("ready"),
                init_calls: ready_init_calls.clone(),
            }),
        );
        engine.push_as(
            "missing-service",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-service")
                    .require_service::<TestCounterService>(),
                init_calls: missing_service_init_calls.clone(),
            }),
        );
        engine.push_as(
            "missing-capability",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-capability")
                    .require_capability(Capability::Reaction),
                init_calls: missing_capability_init_calls.clone(),
            }),
        );

        let err = engine.init_all().await.unwrap_err();
        let err = err.to_string();

        assert_eq!(*ready_init_calls.lock().unwrap(), 0);
        assert_eq!(*missing_service_init_calls.lock().unwrap(), 0);
        assert_eq!(*missing_capability_init_calls.lock().unwrap(), 0);
        assert!(err.contains("missing-service"));
        assert!(err.contains(std::any::type_name::<TestCounterService>()));
        assert!(err.contains("missing-capability"));
        assert!(err.contains("Reaction"));
        assert_eq!(
            state.snapshot("ready").lifecycle_state,
            PluginLifecycleState::Registered
        );
        assert_eq!(
            state.snapshot("missing-service").lifecycle_state,
            PluginLifecycleState::Failed
        );
        assert_eq!(
            state.snapshot("missing-capability").lifecycle_state,
            PluginLifecycleState::Failed
        );
    }

    #[test]
    fn runtime_plugin_engine_reports_all_startup_requirement_failures() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "missing-service",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-service")
                    .require_service::<TestCounterService>(),
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );
        engine.push_as(
            "missing-capability",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-capability")
                    .require_capability(Capability::Reaction),
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );

        let failures = engine.validate_startup_requirements();

        assert_eq!(
            failures,
            vec![
                PluginRequirementFailure {
                    instance_id: "missing-service".to_string(),
                    missing_capabilities: Vec::new(),
                    missing_services: vec![ServiceKey::of::<TestCounterService>()],
                },
                PluginRequirementFailure {
                    instance_id: "missing-capability".to_string(),
                    missing_capabilities: vec![Capability::Reaction],
                    missing_services: Vec::new(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn runtime_plugin_engine_does_not_init_any_plugin_when_preflight_fails() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let ready_init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "ready",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("ready"),
                init_calls: ready_init_calls.clone(),
            }),
        );
        engine.push_as(
            "missing-service",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-service")
                    .require_service::<TestCounterService>(),
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );

        let err = engine.init_all().await.unwrap_err();

        assert!(err.to_string().contains("missing-service"));
        assert_eq!(*ready_init_calls.lock().unwrap(), 0);
        assert_eq!(
            state.snapshot("ready").lifecycle_state,
            PluginLifecycleState::Registered
        );
    }

    #[test]
    fn startup_requirement_validation_is_read_only() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "missing-service",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("missing-service")
                    .require_service::<TestCounterService>(),
                init_calls: init_calls.clone(),
            }),
        );

        let before = state.snapshot("missing-service");
        let failures = engine.validate_startup_requirements();
        let after = state.snapshot("missing-service");

        assert_eq!(failures.len(), 1);
        assert_eq!(before, after);
        assert_eq!(*init_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_orders_handlers_by_priority_and_respects_block() {
        let host = PluginHost::new(None);
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
        let host = PluginHost::new(None);
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
                user_id: "user".to_string(),
                group_id: Some("g1".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["regex"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_uses_context_ids_for_permission_checks() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push(Box::new(PriorityPlugin {
            instance_id: "user-guard",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message().require_permission(Permission::user("admin")),
            manifest: RuntimePluginManifest::new("user-guard"),
            hits: hits.clone(),
        }));
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        engine
            .handle_all(&TestCtx {
                text: "admin said hello".to_string(),
                user_id: "guest".to_string(),
                group_id: None,
            })
            .await
            .unwrap();

        engine
            .handle_all(&TestCtx {
                text: "plain text".to_string(),
                user_id: "admin".to_string(),
                group_id: None,
            })
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["user-guard"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_supports_dry_run_config_lifecycle() {
        let host = PluginHost::new(None);
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
        let host = PluginHost::new(None);
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

    #[tokio::test]
    async fn runtime_plugin_engine_accepts_declared_services_capabilities() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host)
            .with_capabilities([Capability::GroupModeration, Capability::Reaction]);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(PriorityPlugin {
            instance_id: "moderator",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("moderator")
                .require_capability(Capability::GroupModeration),
            hits,
        }));

        engine.init_all().await.unwrap();
        let snapshot = state.snapshot("moderator");
        assert_ne!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert!(snapshot.last_error.is_none());
    }

    #[tokio::test]
    async fn engine_can_register_plugin_with_explicit_instance_id() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "custom.instance",
            Box::new(PriorityPlugin {
                instance_id: "macro-default",
                priority: 0,
                block_decl: false,
                handler: HandlerDecl::wildcard_message(),
                manifest: RuntimePluginManifest::new("named"),
                hits,
            }),
        );
        engine.init_all().await.unwrap();

        let snapshots = state.snapshots();
        assert!(snapshots.iter().any(|(id, _)| id == "custom.instance"));
        assert!(!snapshots.iter().any(|(id, _)| id == "macro-default"));
    }

    struct ServiceProbePlugin {
        seen_instance_ids: Arc<std::sync::Mutex<Vec<Option<String>>>>,
    }

    #[async_trait]
    impl RuntimePlugin<TestCtx> for ServiceProbePlugin {
        fn kind(&self) -> &'static str {
            "service-probe"
        }

        fn meta(&self) -> PluginMetadata {
            PluginMetadata::new("service-probe")
        }

        async fn init(&mut self, services: RuntimePluginServices<TestCtx>) -> Result<()> {
            self.seen_instance_ids
                .lock()
                .unwrap()
                .push(services.instance_id);
            Ok(())
        }

        async fn start(&mut self, services: RuntimePluginServices<TestCtx>) -> Result<()> {
            self.seen_instance_ids
                .lock()
                .unwrap()
                .push(services.instance_id);
            Ok(())
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    #[tokio::test]
    async fn engine_scopes_services_to_runtime_instance_id() {
        let host = PluginHost::new(None);
        let services = RuntimePluginServices::new(host);
        let state = PluginRuntimeState::default();
        let seen_instance_ids = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "runtime-id",
            Box::new(ServiceProbePlugin {
                seen_instance_ids: seen_instance_ids.clone(),
            }),
        );

        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        assert_eq!(
            *seen_instance_ids.lock().unwrap(),
            vec![
                Some("runtime-id".to_string()),
                Some("runtime-id".to_string())
            ]
        );
    }
}
