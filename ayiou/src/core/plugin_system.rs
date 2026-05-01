use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::{
    command::parse_command_line,
    model::{BotId, CommandInvocation, PlatformId},
    plugin_host::PluginHost,
    plugin_runtime::{PluginLifecycleState, PluginRuntimeState},
};

#[derive(Clone, Debug)]
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
}

impl<C> Clone for RuntimePluginServices<C> {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            instance_id: self.instance_id.clone(),
            bot_id: self.bot_id.clone(),
            platform: self.platform.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}

impl<C> RuntimePluginServices<C> {
    #[must_use]
    pub const fn new(host: PluginHost<C>) -> Self {
        Self {
            host,
            instance_id: None,
            bot_id: None,
            platform: None,
            capabilities: Vec::new(),
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

    pub async fn init_all(&mut self) -> Result<()> {
        let provided_capabilities = self.services.provided_capabilities();
        for registered in &mut self.plugins {
            let instance_id = registered.instance_id().to_string();
            self.runtime_state
                .set_lifecycle(&instance_id, PluginLifecycleState::Initializing);

            if let CapabilityNegotiation::Failed { missing_required } =
                negotiate_capabilities(&registered.plugin().manifest(), &provided_capabilities)
            {
                let err = anyhow!(
                    "plugin `{instance_id}` missing required capabilities: {missing_required:?}"
                );
                self.runtime_state
                    .record_error(&instance_id, err.to_string());
                return Err(err);
            }

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

    use crate::core::{adapter::MsgContext, plugin_host::PluginHost};

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
