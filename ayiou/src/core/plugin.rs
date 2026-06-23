use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;

use crate::core::{
    command::parse_command_line_with_prefixes,
    context::Context,
    model::{BotId, ChannelRef, CommandInvocation, OutboundMessage, OutboundReceipt, PlatformId},
    service::{RuntimeService, ServiceDescriptor, ServiceKey, ServiceRegistry, ServiceSnapshot},
};

pub(crate) fn normalize_command_prefixes(
    command_prefixes: impl IntoIterator<Item = impl Into<String>>,
) -> Arc<[String]> {
    let mut prefixes: Vec<String> = command_prefixes
        .into_iter()
        .map(Into::into)
        .filter(|prefix| !prefix.is_empty())
        .collect();
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    prefixes.dedup();
    prefixes.into()
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
pub struct CommandMeta {
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub usage: String,
    pub examples: Vec<String>,
}

impl CommandMeta {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            summary: String::new(),
            usage: String::new(),
            examples: Vec::new(),
        }
    }

    #[must_use]
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    #[must_use]
    pub fn usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = usage.into();
        self
    }

    #[must_use]
    pub fn examples(mut self, examples: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.examples = examples.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerDecl {
    pub event_kind: HandlerEventKind,
    pub commands: Vec<String>,
    pub command_prefixes: Vec<String>,
    pub regex_patterns: Vec<String>,
    pub permissions: Vec<Permission>,
    pub command_meta: Vec<CommandMeta>,
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
            command_meta: Vec::new(),
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
            command_meta: Vec::new(),
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
            command_meta: Vec::new(),
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
    pub fn require_permissions(
        mut self,
        permissions: impl IntoIterator<Item = Permission>,
    ) -> Self {
        self.permissions.extend(permissions);
        self
    }

    #[must_use]
    pub fn command_meta(mut self, meta: impl IntoIterator<Item = CommandMeta>) -> Self {
        self.command_meta = meta.into_iter().collect();
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

    #[must_use]
    pub const fn concurrency(mut self, concurrency: ConcurrencyPolicy) -> Self {
        self.concurrency = concurrency;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    Any,
    User(String),
    Group(String),
    Bot(String),
    Role(String),
    Custom(String),
    PlatformCapability(Capability),
}

impl Permission {
    pub fn user(user_id: impl Into<String>) -> Self {
        Self::User(user_id.into())
    }

    pub fn group(group_id: impl Into<String>) -> Self {
        Self::Group(group_id.into())
    }

    pub fn role(role: impl Into<String>) -> Self {
        Self::Role(role.into())
    }

    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConcurrencyPolicy {
    #[default]
    Parallel,
    PluginSerial,
    UserSerial,
    GroupSerial,
    ConversationSerial,
    Drop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigUpdate {
    pub version: u64,
    pub values: serde_json::Value,
    pub dry_run: bool,
}

impl ConfigUpdate {
    pub fn new(version: u64, values: impl Into<serde_json::Value>) -> Self {
        let values = values.into();
        Self {
            version,
            values,
            dry_run: false,
        }
    }

    pub fn dry_run(version: u64, values: impl Into<serde_json::Value>) -> Self {
        let values = values.into();
        Self {
            version,
            values,
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

#[async_trait]
pub trait OutboundSender: Send + Sync {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny(String),
}

impl PermissionDecision {
    #[must_use]
    pub const fn allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[async_trait]
pub trait PermissionService: RuntimeService {
    async fn check(&self, ctx: &Context, permission: &Permission) -> Result<PermissionDecision>;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConversationKey {
    pub plugin_id: String,
    pub user_id: String,
    pub group_id: Option<String>,
}

impl ConversationKey {
    pub fn new(
        plugin_id: impl Into<String>,
        user_id: impl Into<String>,
        group_id: Option<impl Into<String>>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            user_id: user_id.into(),
            group_id: group_id.map(Into::into),
        }
    }

    #[must_use]
    pub fn from_context(plugin_id: impl Into<String>, ctx: &Context) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            user_id: ctx.user_id().into_owned(),
            group_id: ctx.group_id().map(std::borrow::Cow::into_owned),
        }
    }
}

#[async_trait]
pub trait ConversationStore: RuntimeService {
    async fn get(&self, key: &ConversationKey) -> Result<Option<serde_json::Value>>;
    async fn put(
        &self,
        key: ConversationKey,
        value: serde_json::Value,
        ttl: Option<Duration>,
    ) -> Result<()>;
    async fn remove(&self, key: &ConversationKey) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryConversationStore {
    entries: DashMap<ConversationKey, ConversationEntry>,
}

struct ConversationEntry {
    value: serde_json::Value,
    expires_at: Option<Instant>,
}

impl RuntimeService for MemoryConversationStore {
    fn name(&self) -> &'static str {
        "memory-conversation-store"
    }
}

#[async_trait]
impl ConversationStore for MemoryConversationStore {
    async fn get(&self, key: &ConversationKey) -> Result<Option<serde_json::Value>> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(None);
        };
        if entry
            .expires_at
            .is_some_and(|expires_at| expires_at <= Instant::now())
        {
            drop(entry);
            self.entries.remove(key);
            return Ok(None);
        }
        Ok(Some(entry.value.clone()))
    }

    async fn put(
        &self,
        key: ConversationKey,
        value: serde_json::Value,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let expires_at = ttl.map(|ttl| Instant::now() + ttl);
        self.entries
            .insert(key, ConversationEntry { value, expires_at });
        Ok(())
    }

    async fn remove(&self, key: &ConversationKey) -> Result<()> {
        self.entries.remove(key);
        Ok(())
    }
}

#[derive(Clone)]
pub struct RuntimePluginServices {
    sender: Option<Arc<dyn OutboundSender>>,
    permission_service: Option<Arc<dyn PermissionService>>,
    pub instance_id: Option<String>,
    pub bot_id: Option<BotId>,
    pub platform: Option<PlatformId>,
    pub capabilities: Vec<Capability>,
    pub service_registry: ServiceRegistry,
}

impl RuntimePluginServices {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sender: None,
            permission_service: None,
            instance_id: None,
            bot_id: None,
            platform: None,
            capabilities: Vec::new(),
            service_registry: ServiceRegistry::default(),
        }
    }

    #[must_use]
    pub fn with_sender(mut self, sender: Option<Arc<dyn OutboundSender>>) -> Self {
        self.sender = sender;
        self
    }

    #[must_use]
    pub fn with_permission_service(mut self, service: Option<Arc<dyn PermissionService>>) -> Self {
        self.permission_service = service;
        self
    }

    #[must_use]
    pub fn permission_checker(&self) -> Option<Arc<dyn PermissionService>> {
        self.permission_service.clone()
    }

    #[must_use]
    pub fn sender(&self) -> Option<Arc<dyn OutboundSender>> {
        self.sender.clone()
    }

    pub fn require_sender(&self) -> Result<Arc<dyn OutboundSender>> {
        self.sender
            .clone()
            .ok_or_else(|| anyhow!("adapter does not provide proactive message sending"))
    }

    pub async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| anyhow!("adapter does not provide proactive message sending"))?;
        sender.send(message).await
    }

    pub async fn send_text(
        &self,
        target: ChannelRef,
        text: impl Into<String>,
    ) -> Result<OutboundReceipt> {
        self.send(OutboundMessage::text(target, text)).await
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
    pub fn permission_service<S>(&self) -> Option<Arc<S>>
    where
        S: PermissionService,
    {
        self.service_registry.get::<S>()
    }

    pub fn require_permission_service<S>(&self) -> Result<Arc<S>>
    where
        S: PermissionService,
    {
        self.service_registry.require::<S>()
    }

    #[must_use]
    pub fn conversation_store<S>(&self) -> Option<Arc<S>>
    where
        S: ConversationStore,
    {
        self.service_registry.get::<S>()
    }

    pub fn require_conversation_store<S>(&self) -> Result<Arc<S>>
    where
        S: ConversationStore,
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
    pub fn service_snapshot<S>(&self) -> Option<ServiceSnapshot>
    where
        S: RuntimeService,
    {
        self.service_registry.snapshot::<S>()
    }

    #[must_use]
    pub fn service_snapshots(&self) -> Vec<ServiceSnapshot> {
        self.service_registry.snapshots()
    }

    #[must_use]
    pub fn provided_capabilities(&self) -> Vec<Capability> {
        let mut capabilities = self.capabilities.clone();
        if self.sender.is_some() && !capabilities.contains(&Capability::ProactiveSend) {
            capabilities.push(Capability::ProactiveSend);
        }
        capabilities
    }
}

impl Default for RuntimePluginServices {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PluginLifecycleState {
    #[default]
    Registered,
    Initializing,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConfigLifecycleState {
    Draft,
    Validated,
    #[default]
    Applied,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginInstanceState {
    pub enabled: bool,
    pub desired_config_version: u64,
    pub applied_config_version: u64,
    pub config_lifecycle_state: ConfigLifecycleState,
    pub lifecycle_state: PluginLifecycleState,
    pub last_error: Option<String>,
}

impl Default for PluginInstanceState {
    fn default() -> Self {
        Self {
            enabled: true,
            desired_config_version: 0,
            applied_config_version: 0,
            config_lifecycle_state: ConfigLifecycleState::Applied,
            lifecycle_state: PluginLifecycleState::Registered,
            last_error: None,
        }
    }
}

#[derive(Default, Clone)]
pub struct PluginRuntimeState {
    instances: Arc<DashMap<String, PluginInstanceState>>,
}

impl PluginRuntimeState {
    fn update(&self, plugin: &str, f: impl FnOnce(&mut PluginInstanceState)) {
        let mut entry = self.instances.entry(plugin.to_string()).or_default();
        f(entry.value_mut());
    }

    #[must_use]
    pub fn snapshot(&self, plugin: &str) -> PluginInstanceState {
        self.instances
            .get(plugin)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn snapshots(&self) -> Vec<(String, PluginInstanceState)> {
        let mut snapshots: Vec<_> = self
            .instances
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        snapshots.sort_by(|a, b| a.0.cmp(&b.0));
        snapshots
    }

    pub fn set_enabled(&self, plugin: &str, on: bool) {
        self.update(plugin, |state| state.enabled = on);
    }

    #[must_use]
    pub fn is_enabled(&self, plugin: &str) -> bool {
        self.instances
            .get(plugin)
            .map(|entry| entry.enabled)
            .unwrap_or(true)
    }

    pub fn set_desired_config_version(&self, plugin: &str, version: u64) {
        self.update(plugin, |state| {
            state.desired_config_version = version;
            state.config_lifecycle_state = ConfigLifecycleState::Draft;
        });
    }

    pub fn mark_config_validated(&self, plugin: &str, version: u64) {
        self.update(plugin, |state| {
            state.desired_config_version = state.desired_config_version.max(version);
            state.config_lifecycle_state = ConfigLifecycleState::Validated;
        });
    }

    pub fn mark_config_applied(&self, plugin: &str, version: u64) {
        self.update(plugin, |state| {
            state.applied_config_version = version;
            state.desired_config_version = state.desired_config_version.max(version);
            state.config_lifecycle_state = ConfigLifecycleState::Applied;
        });
    }

    pub fn reject_config(&self, plugin: &str, version: u64, error: impl Into<String>) {
        self.update(plugin, |state| {
            state.desired_config_version = state.desired_config_version.max(version);
            state.config_lifecycle_state = ConfigLifecycleState::Rejected;
            state.last_error = Some(error.into());
        });
    }

    pub fn set_lifecycle(&self, plugin: &str, lifecycle_state: PluginLifecycleState) {
        self.update(plugin, |state| state.lifecycle_state = lifecycle_state);
    }

    pub fn record_error(&self, plugin: &str, error: impl Into<String>) {
        self.update(plugin, |state| {
            state.lifecycle_state = PluginLifecycleState::Failed;
            state.last_error = Some(error.into());
        });
    }

    pub fn clear_error(&self, plugin: &str) {
        self.update(plugin, |state| state.last_error = None);
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
    pub manifest: RuntimePluginManifest,
    pub lifecycle: PluginInstanceState,
    pub health: PluginHealth,
    pub reloadable: bool,
}

#[async_trait]
pub trait PluginReloader: Send + Sync + 'static {
    async fn reload(&self, instance_id: &str) -> Result<RegisteredPlugin>;
}

#[derive(Clone, Default)]
pub enum PluginReloadDescriptor {
    #[default]
    NotReloadable,
    Reloadable(Arc<dyn PluginReloader>),
}

impl PluginReloadDescriptor {
    #[must_use]
    pub const fn is_reloadable(&self) -> bool {
        matches!(self, Self::Reloadable(_))
    }
}

#[async_trait]
pub trait RuntimePlugin: Send + Sync + 'static {
    fn kind(&self) -> &str;

    fn manifest(&self) -> RuntimePluginManifest {
        RuntimePluginManifest::new(self.kind())
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        Vec::new()
    }

    fn register_services(&mut self, _registry: &mut ServiceRegistry) -> Result<()> {
        Ok(())
    }

    async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn start(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, _update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        Ok(ApplyConfigOutcome::skipped())
    }

    async fn handle(&self, ctx: &Context) -> Result<HandleOutcome>;

    async fn handle_with_invocation(
        &self,
        ctx: &Context,
        invocation: Option<CommandInvocation>,
    ) -> Result<HandleOutcome> {
        let _ = invocation;
        self.handle(ctx).await
    }

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}

pub struct RegisteredPlugin {
    instance_id: String,
    handlers: Arc<[HandlerDecl]>,
    plugin: Box<dyn RuntimePlugin>,
    reload: PluginReloadDescriptor,
}

impl RegisteredPlugin {
    pub fn new(instance_id: impl Into<String>, plugin: Box<dyn RuntimePlugin>) -> Self {
        let handlers = plugin.declared_handlers().into();
        Self {
            instance_id: instance_id.into(),
            handlers,
            plugin,
            reload: PluginReloadDescriptor::NotReloadable,
        }
    }

    #[must_use]
    pub fn from_plugin(plugin: Box<dyn RuntimePlugin>) -> Self {
        let instance_id = plugin.kind().to_string();
        Self::new(instance_id, plugin)
    }

    #[must_use]
    pub fn with_reloader(mut self, reloader: Arc<dyn PluginReloader>) -> Self {
        self.reload = PluginReloadDescriptor::Reloadable(reloader);
        self
    }

    #[must_use]
    pub fn reload_descriptor(&self) -> &PluginReloadDescriptor {
        &self.reload
    }

    #[must_use]
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    #[must_use]
    pub fn plugin(&self) -> &dyn RuntimePlugin {
        self.plugin.as_ref()
    }

    #[must_use]
    pub fn handlers(&self) -> &[HandlerDecl] {
        &self.handlers
    }

    pub fn plugin_mut(&mut self) -> &mut dyn RuntimePlugin {
        self.plugin.as_mut()
    }
}

pub type PluginFactory = fn() -> Box<dyn RuntimePlugin>;

pub struct PluginRegistration {
    pub instance_id: &'static str,
    pub factory: PluginFactory,
}

inventory::collect!(PluginRegistration);

#[must_use]
pub fn discovered_plugins() -> Vec<RegisteredPlugin> {
    let mut plugins: Vec<_> = inventory::iter::<PluginRegistration>
        .into_iter()
        .map(|registration| {
            RegisteredPlugin::new(registration.instance_id, (registration.factory)())
        })
        .collect();
    plugins.sort_by(|left, right| left.instance_id().cmp(right.instance_id()));
    plugins
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRequirementFailure {
    pub instance_id: String,
    pub missing_capabilities: Vec<Capability>,
    pub missing_services: Vec<ServiceKey>,
}

pub struct RuntimePluginEngine {
    services: RuntimePluginServices,
    runtime_state: PluginRuntimeState,
    command_prefixes: Arc<[String]>,
    routing_table: RoutingTable,
    concurrency_locks: DashMap<ConcurrencyKey, Arc<tokio::sync::Semaphore>>,
    plugins: Vec<RegisteredPlugin>,
    enabled_plugins: HashMap<String, usize>,
    enabled_order: Vec<usize>,
    disabled_plugins: HashMap<String, usize>,
    service_registered: Vec<bool>,
}

#[derive(Default)]
struct RoutingTable {
    command_prefixes: Arc<[String]>,
    commands: HashMap<String, Vec<Route>>,
    wildcard: Vec<Route>,
    regex: Vec<RegexRoute>,
}

#[derive(Clone)]
struct Route {
    plugin_index: usize,
    handler_index: usize,
    priority: i32,
    block: bool,
    concurrency: ConcurrencyPolicy,
    permissions: Arc<[Permission]>,
}

struct RegexRoute {
    route: Route,
    regex: regex::Regex,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ConcurrencyKey {
    Plugin(usize),
    User(String),
    Group(String),
    Conversation {
        user_id: String,
        group_id: Option<String>,
    },
}

fn compare_routes(left: &Route, right: &Route) -> std::cmp::Ordering {
    left.priority
        .cmp(&right.priority)
        .then(left.plugin_index.cmp(&right.plugin_index))
        .then(left.handler_index.cmp(&right.handler_index))
}

fn sort_routes(routes: &mut [Route]) {
    routes.sort_by(compare_routes);
}

impl RuntimePluginEngine {
    #[must_use]
    pub fn new(services: RuntimePluginServices, runtime_state: PluginRuntimeState) -> Self {
        Self::with_options(services, runtime_state, Arc::from([]))
    }

    #[must_use]
    pub fn with_options(
        services: RuntimePluginServices,
        runtime_state: PluginRuntimeState,
        command_prefixes: Arc<[String]>,
    ) -> Self {
        Self {
            services,
            runtime_state,
            command_prefixes,
            routing_table: RoutingTable::default(),
            concurrency_locks: DashMap::new(),
            plugins: Vec::new(),
            enabled_plugins: HashMap::new(),
            enabled_order: Vec::new(),
            disabled_plugins: HashMap::new(),
            service_registered: Vec::new(),
        }
    }

    pub fn push(&mut self, plugin: Box<dyn RuntimePlugin>) {
        self.push_registered(RegisteredPlugin::from_plugin(plugin));
    }

    pub fn push_as(&mut self, instance_id: impl Into<String>, plugin: Box<dyn RuntimePlugin>) {
        self.push_registered(RegisteredPlugin::new(instance_id, plugin));
    }

    pub(crate) fn push_registered(&mut self, plugin: RegisteredPlugin) {
        let instance_id = plugin.instance_id().to_string();
        assert!(
            !self.enabled_plugins.contains_key(&instance_id)
                && !self.disabled_plugins.contains_key(&instance_id),
            "plugin instance `{instance_id}` is already registered"
        );

        let plugin_index = self.plugins.len();
        self.runtime_state
            .set_lifecycle(&instance_id, PluginLifecycleState::Registered);
        if self.runtime_state.is_enabled(&instance_id) {
            self.enabled_plugins.insert(instance_id, plugin_index);
            self.enabled_order.push(plugin_index);
        } else {
            self.disabled_plugins.insert(instance_id, plugin_index);
        }
        self.service_registered.push(false);
        self.plugins.push(plugin);
        self.rebuild_routing_table()
            .expect("plugin handler declarations must be valid");
    }

    fn rebuild_routing_table(&mut self) -> Result<()> {
        let mut command_prefixes: Vec<String> = self.command_prefixes.iter().cloned().collect();
        let mut table = RoutingTable::default();
        for plugin_index in self.enabled_order.iter().copied() {
            let registered = &self.plugins[plugin_index];
            for (handler_index, handler) in registered.handlers().iter().enumerate() {
                let route = Route {
                    plugin_index,
                    handler_index,
                    priority: handler.priority,
                    block: handler.block,
                    concurrency: handler.concurrency,
                    permissions: handler.permissions.clone().into(),
                };
                for command in &handler.commands {
                    table
                        .commands
                        .entry(command.clone())
                        .or_default()
                        .push(route.clone());
                }
                command_prefixes.extend(handler.command_prefixes.iter().cloned());
                if handler.wildcard {
                    table.wildcard.push(route.clone());
                }
                for pattern in &handler.regex_patterns {
                    let regex = regex::Regex::new(pattern).map_err(|err| {
                        anyhow!(
                            "plugin `{}` declared invalid regex `{}`: {}",
                            registered.instance_id(),
                            pattern,
                            err
                        )
                    })?;
                    table.regex.push(RegexRoute {
                        route: route.clone(),
                        regex,
                    });
                }
            }
        }
        table.command_prefixes = normalize_command_prefixes(command_prefixes);
        for routes in table.commands.values_mut() {
            sort_routes(routes);
        }
        sort_routes(&mut table.wildcard);
        table
            .regex
            .sort_by(|left, right| compare_routes(&left.route, &right.route));
        self.routing_table = table;
        Ok(())
    }

    #[must_use]
    pub fn plugins(&self) -> &[RegisteredPlugin] {
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
                    manifest: plugin.manifest(),
                    health: plugin.health(),
                    reloadable: registered.reload_descriptor().is_reloadable(),
                }
            })
            .collect();
        snapshots.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        snapshots
    }

    pub async fn init_all(&mut self) -> Result<()> {
        self.register_plugin_services()?;
        self.rebuild_routing_table()?;
        self.preflight_startup_requirements()?;

        for order_index in 0..self.enabled_order.len() {
            let plugin_index = self.enabled_order[order_index];
            let instance_id = self.plugins[plugin_index].instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Initializing);

            if let Err(err) = self.plugins[plugin_index]
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

    fn register_plugin_services(&mut self) -> Result<()> {
        for order_index in 0..self.enabled_order.len() {
            let plugin_index = self.enabled_order[order_index];
            if self.service_registered[plugin_index] {
                continue;
            }

            let instance_id = self.plugins[plugin_index].instance_id().to_string();
            if let Err(err) = self.plugins[plugin_index]
                .plugin_mut()
                .register_services(&mut self.services.service_registry)
            {
                let err = anyhow!("plugin `{instance_id}` service registration failed: {err}");
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

            self.service_registered[plugin_index] = true;
        }

        Ok(())
    }

    fn preflight_startup_requirements(&self) -> Result<()> {
        let failures = self.validate_startup_requirements();
        if failures.is_empty() {
            return Ok(());
        }

        let messages: Vec<_> = failures
            .iter()
            .map(|failure| {
                let mut parts = Vec::new();
                if !failure.missing_capabilities.is_empty() {
                    parts.push(format!(
                        "missing required capabilities: {:?}",
                        failure.missing_capabilities
                    ));
                }
                if !failure.missing_services.is_empty() {
                    let missing_names: Vec<_> = failure
                        .missing_services
                        .iter()
                        .map(ServiceKey::type_name)
                        .collect();
                    parts.push(format!("missing required services: {missing_names:?}"));
                }

                format!("plugin `{}` {}", failure.instance_id, parts.join("; "))
            })
            .collect();
        for (failure, message) in failures.iter().zip(&messages) {
            self.runtime_state
                .record_error(&failure.instance_id, message.clone());
        }

        Err(anyhow!("{}", messages.join("; ")))
    }

    #[must_use]
    pub fn validate_startup_requirements(&self) -> Vec<PluginRequirementFailure> {
        let provided_capabilities = self.services.provided_capabilities();
        self.enabled_order
            .iter()
            .copied()
            .filter_map(|plugin_index| {
                let registered = &self.plugins[plugin_index];
                let manifest = registered.plugin().manifest();
                let missing_capabilities =
                    if let CapabilityNegotiation::Failed { missing_required } =
                        negotiate_capabilities(&manifest, &provided_capabilities)
                    {
                        missing_required
                    } else {
                        Vec::new()
                    };
                let missing_services: Vec<ServiceKey> = manifest
                    .required_services
                    .iter()
                    .filter(|service| !self.services.service_registry.contains_key(service))
                    .copied()
                    .collect();

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
        for order_index in 0..self.enabled_order.len() {
            let plugin_index = self.enabled_order[order_index];
            let instance_id = self.plugins[plugin_index].instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Starting);

            if let Err(err) = self.plugins[plugin_index]
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

    pub async fn enable_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        self.disabled_plugins.remove(instance_id);
        if self
            .enabled_plugins
            .insert(instance_id.to_string(), plugin_index)
            .is_none()
        {
            self.enabled_order.push(plugin_index);
        }
        self.runtime_state.set_enabled(instance_id, true);
        self.rebuild_routing_table()?;

        match self.runtime_state.snapshot(instance_id).lifecycle_state {
            PluginLifecycleState::Initializing
            | PluginLifecycleState::Starting
            | PluginLifecycleState::Running => Ok(()),
            PluginLifecycleState::Registered
            | PluginLifecycleState::Stopping
            | PluginLifecycleState::Stopped
            | PluginLifecycleState::Failed => self.start_plugin(instance_id).await,
        }
    }

    pub async fn disable_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        if self.enabled_plugins.remove(instance_id).is_some() {
            self.enabled_order
                .retain(|enabled| *enabled != plugin_index);
        }
        self.disabled_plugins
            .insert(instance_id.to_string(), plugin_index);
        self.runtime_state.set_enabled(instance_id, false);
        self.rebuild_routing_table()?;

        match self.runtime_state.snapshot(instance_id).lifecycle_state {
            PluginLifecycleState::Starting | PluginLifecycleState::Running => {
                self.stop_plugin(instance_id).await
            }
            PluginLifecycleState::Registered
            | PluginLifecycleState::Initializing
            | PluginLifecycleState::Stopping
            | PluginLifecycleState::Stopped
            | PluginLifecycleState::Failed => Ok(()),
        }
    }

    pub async fn start_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        if !self.enabled_plugins.contains_key(instance_id) {
            return Ok(());
        }

        if self.runtime_state.snapshot(instance_id).lifecycle_state
            == PluginLifecycleState::Registered
        {
            self.init_plugin(instance_id).await?;
        }

        let services = self
            .services
            .clone()
            .with_instance_id(instance_id.to_string());
        self.runtime_state
            .set_lifecycle(instance_id, PluginLifecycleState::Starting);

        let start_result = self.plugins[plugin_index]
            .plugin_mut()
            .start(services)
            .await;
        if let Err(err) = start_result {
            self.runtime_state
                .record_error(instance_id, err.to_string());
            return Err(err);
        }

        self.runtime_state
            .set_lifecycle(instance_id, PluginLifecycleState::Running);
        self.runtime_state.clear_error(instance_id);
        Ok(())
    }

    pub async fn init_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        self.register_plugin_services()?;
        self.preflight_startup_requirements()?;

        let services = self
            .services
            .clone()
            .with_instance_id(instance_id.to_string());
        self.runtime_state
            .set_lifecycle(instance_id, PluginLifecycleState::Initializing);

        let init_result = self.plugins[plugin_index].plugin_mut().init(services).await;
        if let Err(err) = init_result {
            self.runtime_state
                .record_error(instance_id, err.to_string());
            return Err(err);
        }

        self.runtime_state.clear_error(instance_id);
        Ok(())
    }

    pub async fn stop_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        self.runtime_state
            .set_lifecycle(instance_id, PluginLifecycleState::Stopping);

        let stop_result = self.plugins[plugin_index].plugin_mut().stop().await;
        if let Err(err) = stop_result {
            self.runtime_state
                .record_error(instance_id, err.to_string());
            return Err(err);
        }

        self.runtime_state
            .set_lifecycle(instance_id, PluginLifecycleState::Stopped);
        self.runtime_state.clear_error(instance_id);
        Ok(())
    }

    pub async fn reload_plugin(&mut self, instance_id: &str) -> Result<()> {
        let plugin_index = self.plugin_index(instance_id)?;
        let PluginReloadDescriptor::Reloadable(reloader) =
            self.plugins[plugin_index].reload_descriptor().clone()
        else {
            self.runtime_state.record_error(
                instance_id,
                format!("plugin `{instance_id}` is not reloadable"),
            );
            return Err(anyhow!("plugin `{instance_id}` is not reloadable"));
        };

        let candidate = match reloader.reload(instance_id).await {
            Ok(candidate) => candidate,
            Err(err) => {
                self.runtime_state
                    .record_error(instance_id, err.to_string());
                return Err(err);
            }
        };
        if candidate.instance_id() != instance_id {
            let err = anyhow!(
                "reload candidate instance `{}` does not match `{instance_id}`",
                candidate.instance_id()
            );
            self.runtime_state
                .record_error(instance_id, err.to_string());
            return Err(err);
        }

        let was_enabled = self.enabled_plugins.contains_key(instance_id);
        let was_running = self.runtime_state.snapshot(instance_id).lifecycle_state
            == PluginLifecycleState::Running;
        let old = std::mem::replace(&mut self.plugins[plugin_index], candidate);
        if let Err(err) = self.rebuild_routing_table() {
            self.plugins[plugin_index] = old;
            let _ = self.rebuild_routing_table();
            self.runtime_state
                .record_error(instance_id, err.to_string());
            return Err(err);
        }

        if was_enabled {
            if let Err(err) = self.init_plugin(instance_id).await {
                self.plugins[plugin_index] = old;
                let _ = self.rebuild_routing_table();
                return Err(err);
            }
            if was_running && let Err(err) = self.start_plugin(instance_id).await {
                self.plugins[plugin_index] = old;
                let _ = self.rebuild_routing_table();
                return Err(err);
            }
        }
        let mut old = old;
        let _ = old.plugin_mut().stop().await;
        self.runtime_state.clear_error(instance_id);
        Ok(())
    }

    fn plugin_index(&self, instance_id: &str) -> Result<usize> {
        self.enabled_plugins
            .get(instance_id)
            .or_else(|| self.disabled_plugins.get(instance_id))
            .copied()
            .ok_or_else(|| anyhow!("plugin instance `{instance_id}` is not registered"))
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
        let plugin_index = self.plugin_index(instance_id)?;

        self.runtime_state
            .set_desired_config_version(instance_id, update.version);
        if update.dry_run {
            self.runtime_state
                .mark_config_validated(instance_id, update.version);
            self.runtime_state.clear_error(instance_id);
            return Ok(ApplyConfigOutcome::skipped());
        }
        let outcome = self.plugins[plugin_index]
            .plugin_mut()
            .apply_config(update)
            .await?;

        if let Some(version) = outcome.applied_version {
            self.runtime_state.mark_config_applied(instance_id, version);
        }

        self.runtime_state.clear_error(instance_id);
        Ok(outcome)
    }
}

impl RuntimePluginEngine {
    pub async fn handle_all(&self, ctx: &Context) -> Result<bool> {
        let text = ctx.text();
        let invocation = parse_command_line_with_prefixes(
            &text,
            self.routing_table
                .command_prefixes
                .iter()
                .map(String::as_str),
        )
        .or_else(|| parse_command_line_with_prefixes(&text, std::iter::empty::<&str>()));

        let mut matched = Vec::new();
        if let Some(invocation) = invocation
            && let Some(routes) = self.routing_table.commands.get(invocation.command())
        {
            matched.extend(routes.iter().cloned().map(|route| MatchedHandler {
                invocation: Some(invocation.clone()),
                match_rank: 0,
                route,
            }));
        }

        if matched.is_empty() {
            matched.extend(self.routing_table.wildcard.iter().cloned().map(|route| {
                MatchedHandler {
                    invocation: None,
                    match_rank: 1,
                    route,
                }
            }));
        }

        matched.extend(
            self.routing_table
                .regex
                .iter()
                .filter(|route| route.regex.is_match(&text))
                .map(|route| MatchedHandler {
                    invocation: None,
                    match_rank: 2,
                    route: route.route.clone(),
                }),
        );

        matched.sort_by(|left, right| {
            left.route
                .priority
                .cmp(&right.route.priority)
                .then(left.match_rank.cmp(&right.match_rank))
                .then(left.route.plugin_index.cmp(&right.route.plugin_index))
                .then(left.route.handler_index.cmp(&right.route.handler_index))
        });

        for candidate in matched {
            let plugin_index = candidate.route.plugin_index;
            let registered = &self.plugins[plugin_index];
            if !self.permissions_match(ctx, &candidate.route).await? {
                continue;
            }

            let _permit = self.acquire_concurrency(ctx, &candidate.route).await?;

            match registered
                .plugin()
                .handle_with_invocation(ctx, candidate.invocation)
                .await
            {
                Ok(outcome) => {
                    self.runtime_state.clear_error(registered.instance_id());
                    if outcome.block || candidate.route.block {
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

    async fn acquire_concurrency(
        &self,
        ctx: &Context,
        route: &Route,
    ) -> Result<Option<tokio::sync::OwnedSemaphorePermit>> {
        let key = match route.concurrency {
            ConcurrencyPolicy::Parallel => return Ok(None),
            ConcurrencyPolicy::Drop => return Ok(None),
            ConcurrencyPolicy::PluginSerial => ConcurrencyKey::Plugin(route.plugin_index),
            ConcurrencyPolicy::UserSerial => ConcurrencyKey::User(ctx.user_id().into_owned()),
            ConcurrencyPolicy::GroupSerial => {
                let Some(group_id) = ctx.group_id() else {
                    return Ok(None);
                };
                ConcurrencyKey::Group(group_id.into_owned())
            }
            ConcurrencyPolicy::ConversationSerial => ConcurrencyKey::Conversation {
                user_id: ctx.user_id().into_owned(),
                group_id: ctx.group_id().map(std::borrow::Cow::into_owned),
            },
        };
        let semaphore = self
            .concurrency_locks
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone();
        Ok(Some(semaphore.acquire_owned().await?))
    }

    async fn permissions_match(&self, ctx: &Context, route: &Route) -> Result<bool> {
        for permission in route.permissions.iter() {
            if !self.permission_matches(ctx, permission).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn permission_matches(&self, ctx: &Context, permission: &Permission) -> Result<bool> {
        let user_id = ctx.user_id();
        let group_id = ctx.group_id();
        match permission {
            Permission::Any => Ok(true),
            Permission::User(allowed_user) => Ok(user_id == *allowed_user),
            Permission::Group(allowed_group) => {
                Ok(group_id.as_deref() == Some(allowed_group.as_str()))
            }
            Permission::Bot(bot_id) => Ok(self
                .services
                .bot_id
                .as_ref()
                .is_some_and(|current| current.as_str() == bot_id)),
            Permission::PlatformCapability(capability) => {
                Ok(self.services.capabilities.contains(capability)
                    || (*capability == Capability::ProactiveSend && self.services.sender.is_some()))
            }
            Permission::Role(_) | Permission::Custom(_) => {
                let Some(service) = self.services.permission_checker() else {
                    return Ok(false);
                };
                Ok(service.check(ctx, permission).await?.allowed())
            }
        }
    }
}

struct MatchedHandler {
    invocation: Option<CommandInvocation>,
    match_rank: u8,
    route: Route,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use crate::core::{
        context::Context,
        model::{
            BotId, ChannelRef, EventEnvelope, OutboundMessage, OutboundReceipt, PlatformId, UserRef,
        },
        plugin::OutboundSender,
        service::{RuntimeService, ServiceKey, ServiceRegistry},
    };

    struct NoopSender;

    #[async_trait]
    impl OutboundSender for NoopSender {
        async fn send(&self, _message: OutboundMessage) -> Result<OutboundReceipt> {
            Ok(OutboundReceipt::default())
        }
    }

    use super::*;

    fn test_ctx(
        text: impl Into<String>,
        user_id: impl Into<String>,
        group_id: Option<&str>,
    ) -> Context {
        let platform = PlatformId::new("test");
        let user = UserRef::new(platform.clone(), user_id.into());
        let channel = match group_id {
            Some(group_id) => ChannelRef::group(platform.clone(), group_id),
            None => ChannelRef::direct(platform.clone(), "direct"),
        };
        let message = crate::core::model::MessageEvent::new(user, channel, text.into());
        Context::new(
            EventEnvelope::new(BotId::new("test-bot"), platform).with_message(message),
            None,
            (),
        )
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

    struct AllowNamedPermission(&'static str);

    impl RuntimeService for AllowNamedPermission {
        fn name(&self) -> &'static str {
            "allow-named-permission"
        }
    }

    #[async_trait]
    impl PermissionService for AllowNamedPermission {
        async fn check(
            &self,
            _ctx: &Context,
            permission: &Permission,
        ) -> Result<PermissionDecision> {
            Ok(match permission {
                Permission::Custom(name) if name == self.0 => PermissionDecision::Allow,
                Permission::Role(name) if name == self.0 => PermissionDecision::Allow,
                _ => PermissionDecision::Deny("permission denied".to_string()),
            })
        }
    }

    struct UnreadyTestService;

    impl RuntimeService for UnreadyTestService {
        fn name(&self) -> &'static str {
            "unready-test"
        }

        fn health(&self) -> crate::core::service::ServiceHealth {
            crate::core::service::ServiceHealth {
                healthy: true,
                ready: false,
                detail: Some("warming up".to_string()),
            }
        }
    }

    #[test]
    fn runtime_plugin_services_provide_registered_services() {
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 7 });
        let services = RuntimePluginServices::new().with_service_registry(registry);

        let service = services
            .require_service::<TestCounterService>()
            .expect("service should be available");

        assert_eq!(service.value, 7);
    }

    #[test]
    fn runtime_plugin_services_describe_registered_services() {
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 7 });
        let services = RuntimePluginServices::new().with_service_registry(registry);

        let descriptor = services
            .service_descriptor::<TestCounterService>()
            .expect("counter service should have a descriptor");

        assert_eq!(descriptor.key, ServiceKey::of::<TestCounterService>());
        assert_eq!(descriptor.name, "test-counter");
        assert_eq!(descriptor.version, "0.1.0");
        assert_eq!(services.service_descriptors(), vec![descriptor]);
    }

    #[test]
    fn runtime_plugin_services_report_service_health_snapshots() {
        let mut registry = ServiceRegistry::default();
        registry.insert(UnreadyTestService);
        let services = RuntimePluginServices::new().with_service_registry(registry);

        let snapshot = services
            .service_snapshot::<UnreadyTestService>()
            .expect("unready service should have a snapshot");

        assert_eq!(snapshot.descriptor.name, "unready-test");
        assert_eq!(
            snapshot.health,
            crate::core::service::ServiceHealth {
                healthy: true,
                ready: false,
                detail: Some("warming up".to_string()),
            }
        );
        assert_eq!(services.service_snapshots(), vec![snapshot]);
    }

    #[test]
    fn runtime_plugin_services_infer_proactive_send_from_host_sender() {
        let sender: Arc<dyn OutboundSender> = Arc::new(NoopSender);
        let services = RuntimePluginServices::new().with_sender(Some(sender));

        assert_eq!(
            services.provided_capabilities(),
            vec![Capability::ProactiveSend]
        );
    }

    #[test]
    fn runtime_state_tracks_versions_and_errors() {
        let state = PluginRuntimeState::default();
        state.set_enabled("echo", false);
        state.set_desired_config_version("echo", 3);
        state.mark_config_applied("echo", 2);
        state.record_error("echo", "boom");

        let snapshot = state.snapshot("echo");
        assert!(!snapshot.enabled);
        assert_eq!(snapshot.desired_config_version, 3);
        assert_eq!(snapshot.applied_config_version, 2);
        assert_eq!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert_eq!(snapshot.last_error.as_deref(), Some("boom"));
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
    impl RuntimePlugin for PriorityPlugin {
        fn kind(&self) -> &str {
            self.instance_id
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

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
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
    impl RuntimePlugin for DependencyPlugin {
        fn kind(&self) -> &'static str {
            "dependency"
        }
        fn manifest(&self) -> RuntimePluginManifest {
            self.manifest.clone()
        }

        async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
            *self.init_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    struct ServiceProviderPlugin {
        service_value: usize,
        fail_registration: bool,
        init_calls: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin for ServiceProviderPlugin {
        fn kind(&self) -> &'static str {
            "service-provider"
        }
        fn register_services(&mut self, registry: &mut ServiceRegistry) -> Result<()> {
            if self.fail_registration {
                return Err(anyhow!("provider registration failed"));
            }
            registry.try_insert(TestCounterService {
                value: self.service_value,
            })
        }

        async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
            *self.init_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    struct ServiceConsumerPlugin {
        observed: Arc<std::sync::Mutex<Vec<usize>>>,
    }

    #[async_trait]
    impl RuntimePlugin for ServiceConsumerPlugin {
        fn kind(&self) -> &'static str {
            "service-consumer"
        }
        fn manifest(&self) -> RuntimePluginManifest {
            RuntimePluginManifest::new("service-consumer").require_service::<TestCounterService>()
        }

        async fn init(&mut self, services: RuntimePluginServices) -> Result<()> {
            let service = services.require_service::<TestCounterService>()?;
            self.observed.lock().unwrap().push(service.value);
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    struct CountingLifecyclePlugin {
        instance_id: &'static str,
        handled: Arc<std::sync::Mutex<usize>>,
        stopped: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin for CountingLifecyclePlugin {
        fn kind(&self) -> &'static str {
            self.instance_id
        }
        fn declared_handlers(&self) -> Vec<HandlerDecl> {
            vec![HandlerDecl::wildcard_message()]
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            *self.handled.lock().unwrap() += 1;
            Ok(HandleOutcome::pass())
        }

        async fn stop(&mut self) -> Result<()> {
            *self.stopped.lock().unwrap() += 1;
            Ok(())
        }
    }

    struct StartCountingPlugin {
        started: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin for StartCountingPlugin {
        fn kind(&self) -> &'static str {
            "switchable"
        }
        async fn start(&mut self, _services: RuntimePluginServices) -> Result<()> {
            *self.started.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    #[derive(Clone)]
    struct TestReloader {
        kind: &'static str,
        handled: Arc<std::sync::Mutex<usize>>,
        stopped: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl PluginReloader for TestReloader {
        async fn reload(&self, instance_id: &str) -> Result<RegisteredPlugin> {
            Ok(RegisteredPlugin::new(
                instance_id.to_string(),
                Box::new(CountingLifecyclePlugin {
                    instance_id: self.kind,
                    handled: self.handled.clone(),
                    stopped: self.stopped.clone(),
                }),
            )
            .with_reloader(Arc::new(self.clone())))
        }
    }

    #[tokio::test]
    async fn disabled_plugin_is_stopped_and_skipped_by_dispatch() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let stopped = Arc::new(std::sync::Mutex::new(0));
        let handled = Arc::new(std::sync::Mutex::new(0));
        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "switchable",
            Box::new(CountingLifecyclePlugin {
                instance_id: "switchable",
                handled: handled.clone(),
                stopped: stopped.clone(),
            }),
        );

        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();
        engine.disable_plugin("switchable").await.unwrap();

        assert!(!state.is_enabled("switchable"));
        assert_eq!(*stopped.lock().unwrap(), 1);
        let blocked = engine
            .handle_all(&test_ctx("", "user", None))
            .await
            .unwrap();
        assert!(!blocked);
        assert_eq!(*handled.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn disabled_plugin_is_not_preflighted_or_initialized() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        state.set_enabled("disabled", false);
        let ready_init_calls = Arc::new(std::sync::Mutex::new(0));
        let disabled_init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "disabled",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("disabled")
                    .require_capability(Capability::Reaction),
                init_calls: disabled_init_calls.clone(),
            }),
        );
        engine.push_as(
            "ready",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("ready"),
                init_calls: ready_init_calls.clone(),
            }),
        );

        engine.init_all().await.unwrap();

        assert_eq!(*ready_init_calls.lock().unwrap(), 1);
        assert_eq!(*disabled_init_calls.lock().unwrap(), 0);
        assert!(!state.is_enabled("disabled"));
        assert_eq!(
            state.snapshot("disabled").lifecycle_state,
            PluginLifecycleState::Registered
        );
    }

    #[tokio::test]
    async fn disabled_plugin_does_not_register_services() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        state.set_enabled("provider", false);

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "provider",
            Box::new(ServiceProviderPlugin {
                service_value: 7,
                fail_registration: false,
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );
        engine.push_as(
            "consumer",
            Box::new(ServiceConsumerPlugin {
                observed: Arc::new(std::sync::Mutex::new(Vec::new())),
            }),
        );

        let err = engine.init_all().await.unwrap_err();

        assert!(err.to_string().contains("missing required services"));
        assert_eq!(
            state.snapshot("provider").lifecycle_state,
            PluginLifecycleState::Registered
        );
        assert_eq!(
            state.snapshot("consumer").lifecycle_state,
            PluginLifecycleState::Failed
        );
    }

    #[tokio::test]
    async fn enable_plugin_restarts_initialized_plugin() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let started = Arc::new(std::sync::Mutex::new(0));
        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "switchable",
            Box::new(StartCountingPlugin {
                started: started.clone(),
            }),
        );

        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();
        engine.disable_plugin("switchable").await.unwrap();
        engine.enable_plugin("switchable").await.unwrap();

        assert!(state.is_enabled("switchable"));
        assert_eq!(*started.lock().unwrap(), 2);
        assert_eq!(
            state.snapshot("switchable").lifecycle_state,
            PluginLifecycleState::Running
        );
    }

    #[tokio::test]
    async fn reload_non_reloadable_plugin_reports_structured_error() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "plain",
            Box::new(DependencyPlugin {
                manifest: RuntimePluginManifest::new("plain"),
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );

        let err = engine.reload_plugin("plain").await.unwrap_err();

        assert!(err.to_string().contains("not reloadable"));
        assert!(
            state
                .snapshot("plain")
                .last_error
                .as_deref()
                .is_some_and(|err| err.contains("not reloadable"))
        );
    }

    #[tokio::test]
    async fn reload_reloadable_plugin_swaps_candidate_atomically() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let old_handled = Arc::new(std::sync::Mutex::new(0));
        let old_stopped = Arc::new(std::sync::Mutex::new(0));
        let new_handled = Arc::new(std::sync::Mutex::new(0));
        let new_stopped = Arc::new(std::sync::Mutex::new(0));
        let reloader = Arc::new(TestReloader {
            kind: "new-kind",
            handled: new_handled.clone(),
            stopped: new_stopped.clone(),
        });
        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_registered(
            RegisteredPlugin::new(
                "reloadable",
                Box::new(CountingLifecyclePlugin {
                    instance_id: "old-kind",
                    handled: old_handled.clone(),
                    stopped: old_stopped.clone(),
                }),
            )
            .with_reloader(reloader),
        );
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        engine.reload_plugin("reloadable").await.unwrap();
        engine
            .handle_all(&test_ctx("hello", "user", None))
            .await
            .unwrap();

        assert_eq!(*old_stopped.lock().unwrap(), 1);
        assert_eq!(*old_handled.lock().unwrap(), 0);
        assert_eq!(*new_handled.lock().unwrap(), 1);
        assert!(engine.plugin_snapshots()[0].reloadable);
        assert_eq!(
            state.snapshot("reloadable").lifecycle_state,
            PluginLifecycleState::Running
        );
    }

    #[tokio::test]
    async fn plugin_provided_service_satisfies_consumer_manifest() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let provider_init_calls = Arc::new(std::sync::Mutex::new(0));
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "provider",
            Box::new(ServiceProviderPlugin {
                service_value: 99,
                fail_registration: false,
                init_calls: provider_init_calls.clone(),
            }),
        );
        engine.push_as(
            "consumer",
            Box::new(ServiceConsumerPlugin {
                observed: observed.clone(),
            }),
        );

        engine.init_all().await.unwrap();

        assert_eq!(*provider_init_calls.lock().unwrap(), 1);
        assert_eq!(*observed.lock().unwrap(), vec![99]);
    }

    #[tokio::test]
    async fn plugin_service_registration_runs_before_dependency_preflight() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "consumer",
            Box::new(ServiceConsumerPlugin {
                observed: observed.clone(),
            }),
        );
        engine.push_as(
            "provider",
            Box::new(ServiceProviderPlugin {
                service_value: 7,
                fail_registration: false,
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );

        engine.init_all().await.unwrap();

        assert_eq!(*observed.lock().unwrap(), vec![7]);
    }

    #[tokio::test]
    async fn duplicate_plugin_provided_service_fails_startup() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let first_init_calls = Arc::new(std::sync::Mutex::new(0));
        let second_init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "provider-a",
            Box::new(ServiceProviderPlugin {
                service_value: 1,
                fail_registration: false,
                init_calls: first_init_calls.clone(),
            }),
        );
        engine.push_as(
            "provider-b",
            Box::new(ServiceProviderPlugin {
                service_value: 2,
                fail_registration: false,
                init_calls: second_init_calls.clone(),
            }),
        );

        let err = engine.init_all().await.unwrap_err();

        assert!(err.to_string().contains("runtime service"));
        assert!(
            err.to_string()
                .contains(std::any::type_name::<TestCounterService>())
        );
        assert_eq!(*first_init_calls.lock().unwrap(), 0);
        assert_eq!(*second_init_calls.lock().unwrap(), 0);
        assert_eq!(
            state.snapshot("provider-b").lifecycle_state,
            PluginLifecycleState::Failed
        );
    }

    #[tokio::test]
    async fn plugin_service_registration_failure_prevents_partial_init() {
        let services = RuntimePluginServices::new();
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
            "provider",
            Box::new(ServiceProviderPlugin {
                service_value: 1,
                fail_registration: true,
                init_calls: Arc::new(std::sync::Mutex::new(0)),
            }),
        );

        let err = engine.init_all().await.unwrap_err();

        assert!(err.to_string().contains("provider registration failed"));
        assert_eq!(*ready_init_calls.lock().unwrap(), 0);
        assert_eq!(
            state.snapshot("provider").lifecycle_state,
            PluginLifecycleState::Failed
        );
        assert_eq!(
            state.snapshot("ready").lifecycle_state,
            PluginLifecycleState::Registered
        );
    }

    struct SnapshotPlugin {
        kind: &'static str,
        manifest: RuntimePluginManifest,
        health: PluginHealth,
        init_calls: Arc<std::sync::Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin for SnapshotPlugin {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn manifest(&self) -> RuntimePluginManifest {
            self.manifest.clone()
        }

        async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
            *self.init_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }

        fn health(&self) -> PluginHealth {
            self.health.clone()
        }
    }

    #[test]
    fn runtime_plugin_engine_reports_plugin_snapshots() {
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        state.set_enabled("snapshot.instance", false);
        state.set_desired_config_version("snapshot.instance", 12);
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as(
            "snapshot.instance",
            Box::new(SnapshotPlugin {
                kind: "snapshot-kind",
                manifest: RuntimePluginManifest::new("snapshot-kind")
                    .description("snapshot plugin")
                    .version("9.9.9")
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
        assert_eq!(snapshot.manifest.kind, "snapshot-kind");
        assert_eq!(snapshot.manifest.description, "snapshot plugin");
        assert_eq!(snapshot.manifest.version, "9.9.9");
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
            ConfigLifecycleState::Draft
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push_as(
            "snapshot-only",
            Box::new(SnapshotPlugin {
                kind: "snapshot-only",
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
        let services = RuntimePluginServices::new();
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
        let mut registry = ServiceRegistry::default();
        registry.insert(TestCounterService { value: 1 });
        let services = RuntimePluginServices::new().with_service_registry(registry);
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
    async fn required_service_readiness_policy_is_explicit() {
        let mut registry = ServiceRegistry::default();
        registry.insert(UnreadyTestService);
        let services = RuntimePluginServices::new().with_service_registry(registry);
        let state = PluginRuntimeState::default();
        let init_calls = Arc::new(std::sync::Mutex::new(0));

        let mut engine = RuntimePluginEngine::new(services, state.clone());
        engine.push(Box::new(DependencyPlugin {
            manifest: RuntimePluginManifest::new("dependency")
                .require_service::<UnreadyTestService>(),
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new();
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

        let blocked = engine
            .handle_all(&test_ctx("", "user", None))
            .await
            .unwrap();

        assert!(blocked);
        assert_eq!(*hits.lock().unwrap(), vec!["first"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_filters_by_regex_and_permission() {
        let services = RuntimePluginServices::new();
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
            .handle_all(&test_ctx("hello world", "user", Some("g1")))
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["regex"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_uses_context_ids_for_permission_checks() {
        let services = RuntimePluginServices::new();
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
            .handle_all(&test_ctx("admin said hello", "guest", None))
            .await
            .unwrap();

        engine
            .handle_all(&test_ctx("plain text", "admin", None))
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["user-guard"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_checks_dynamic_permissions_during_dispatch() {
        let permission_service: Arc<dyn PermissionService> =
            Arc::new(AllowNamedPermission("allowed"));
        let services =
            RuntimePluginServices::new().with_permission_service(Some(permission_service));
        let state = PluginRuntimeState::default();
        let hits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push(Box::new(PriorityPlugin {
            instance_id: "allowed-custom",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message()
                .require_permission(Permission::custom("allowed")),
            manifest: RuntimePluginManifest::new("allowed-custom"),
            hits: hits.clone(),
        }));
        engine.push(Box::new(PriorityPlugin {
            instance_id: "denied-custom",
            priority: 0,
            block_decl: false,
            handler: HandlerDecl::wildcard_message()
                .require_permission(Permission::custom("denied")),
            manifest: RuntimePluginManifest::new("denied-custom"),
            hits: hits.clone(),
        }));
        engine.init_all().await.unwrap();
        engine.start_all().await.unwrap();

        engine
            .handle_all(&test_ctx("plain text", "guest", None))
            .await
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), vec!["allowed-custom"]);
    }

    #[tokio::test]
    async fn runtime_plugin_engine_supports_dry_run_config_lifecycle() {
        let services = RuntimePluginServices::new();
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
            ConfigLifecycleState::Validated
        );
    }

    #[tokio::test]
    async fn runtime_plugin_engine_fails_startup_when_required_capability_is_missing() {
        let services = RuntimePluginServices::new();
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
        let services = RuntimePluginServices::new()
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
        let services = RuntimePluginServices::new();
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
    impl RuntimePlugin for ServiceProbePlugin {
        fn kind(&self) -> &'static str {
            "service-probe"
        }
        async fn init(&mut self, services: RuntimePluginServices) -> Result<()> {
            self.seen_instance_ids
                .lock()
                .unwrap()
                .push(services.instance_id);
            Ok(())
        }

        async fn start(&mut self, services: RuntimePluginServices) -> Result<()> {
            self.seen_instance_ids
                .lock()
                .unwrap()
                .push(services.instance_id);
            Ok(())
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }
    }

    #[tokio::test]
    async fn engine_scopes_services_to_runtime_instance_id() {
        let services = RuntimePluginServices::new();
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
