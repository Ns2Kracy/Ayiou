use std::sync::Arc;

use anyhow::Result;
use log::info;

use crate::adapter::onebot::v11::ctx::Ctx;

// ============================================================================
// Args parsing types
// ============================================================================

/// Error returned when argument parsing fails
#[derive(Debug, Clone)]
pub struct ArgsParseError {
    message: String,
    help: Option<String>,
}

impl ArgsParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn help(&self) -> Option<&str> {
        self.help.as_deref()
    }
}

impl std::fmt::Display for ArgsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArgsParseError {}

/// Trait for parsing command arguments
pub trait Args: Sized + Default {
    /// Parse arguments from a string
    fn parse(args: &str) -> std::result::Result<Self, ArgsParseError>;

    /// Get usage/help text for this args type (optional)
    fn usage() -> Option<&'static str> {
        None
    }
}

impl<T: Args> Args for Box<T> {
    fn parse(args: &str) -> std::result::Result<Self, ArgsParseError> {
        Ok(Box::new(T::parse(args)?))
    }

    fn usage() -> Option<&'static str> {
        T::usage()
    }
}

/// Trait for self-contained command execution
/// Implement this for your Args struct to avoid specifying a handler manually
#[async_trait::async_trait]
pub trait Command: Args + Send + Sync + 'static {
    async fn run(self, ctx: Ctx) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: Command> Command for Box<T> {
    async fn run(self, ctx: Ctx) -> Result<()> {
        (*self).run(ctx).await
    }
}

// ============================================================================
// Cron schedule wrapper
// ============================================================================

/// A parsed cron schedule that can compute upcoming trigger times
#[derive(Clone, Debug)]
pub struct CronSchedule {
    inner: cron::Schedule,
    source: String,
}

impl CronSchedule {
    /// Parse a cron expression string
    pub fn parse(expr: &str) -> std::result::Result<Self, ArgsParseError> {
        use std::str::FromStr;
        cron::Schedule::from_str(expr)
            .map(|inner| Self {
                inner,
                source: expr.to_string(),
            })
            .map_err(|e| ArgsParseError::new(format!("Invalid cron expression: {}", e)))
    }

    /// Get the source cron expression
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get an iterator of upcoming trigger times from now
    pub fn upcoming(&self) -> impl Iterator<Item = chrono::DateTime<chrono::Utc>> + '_ {
        self.inner.upcoming(chrono::Utc)
    }

    /// Get the next trigger time after a given datetime
    pub fn next_after(
        &self,
        after: &chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        self.inner.after(after).next()
    }

    /// Check if a datetime matches this schedule
    pub fn includes(&self, dt: chrono::DateTime<chrono::Utc>) -> bool {
        self.inner.includes(dt)
    }
}

impl std::fmt::Display for CronSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

// ============================================================================
// Regex wrapper for validated strings
// ============================================================================

/// A string that has been validated against a regex pattern
#[derive(Clone, Debug)]
pub struct RegexValidated {
    value: String,
    pattern: &'static str,
}

impl RegexValidated {
    /// Validate a string against a regex pattern
    pub fn validate(
        value: &str,
        pattern: &'static str,
    ) -> std::result::Result<Self, ArgsParseError> {
        let re = regex::Regex::new(pattern)
            .map_err(|e| ArgsParseError::new(format!("Invalid regex pattern: {}", e)))?;
        if re.is_match(value) {
            Ok(Self {
                value: value.to_string(),
                pattern,
            })
        } else {
            Err(ArgsParseError::new(format!(
                "Value '{}' does not match pattern '{}'",
                value, pattern
            )))
        }
    }

    /// Get the validated value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Get the pattern used for validation
    pub fn pattern(&self) -> &'static str {
        self.pattern
    }
}

impl std::fmt::Display for RegexValidated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl AsRef<str> for RegexValidated {
    fn as_ref(&self) -> &str {
        &self.value
    }
}

// ============================================================================
// Plugin metadata
// ============================================================================

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

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

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

/// Plugin trait: main entry point for message handling
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Metadata (name/description/version)
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::default()
    }

    /// Get list of commands this plugin handles (optimization)
    /// If empty, plugin is considered a "wildcard" and checked for every message.
    /// Commands are matched against the first word of the message (whitespace separated).
    fn commands(&self) -> Vec<String> {
        Vec::new()
    }

    /// Check if this plugin matches the context (default: always match)
    fn matches(&self, _ctx: &Ctx) -> bool {
        true
    }

    /// Handle the message, return Ok(true) to block subsequent handlers
    async fn handle(&self, ctx: &Ctx) -> Result<bool>;
}

/// Trait for individual commands
#[async_trait::async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    async fn handle(self, ctx: &Ctx) -> Result<()>;
}

pub type PluginBox = Box<dyn Plugin>;

pub trait IntoPluginBox {
    fn into_plugin_box(self) -> PluginBox;
}

impl<P> IntoPluginBox for P
where
    P: Plugin,
{
    fn into_plugin_box(self) -> PluginBox {
        Box::new(self)
    }
}

impl<P> IntoPluginBox for fn() -> P
where
    P: Plugin,
{
    fn into_plugin_box(self) -> PluginBox {
        Box::new(self())
    }
}

type PluginList = Arc<[Arc<dyn Plugin>]>;

#[derive(Clone)]
pub struct PluginManager {
    /// Plugins pending registration (build phase)
    pending: Vec<Arc<dyn Plugin>>,
    /// Runtime plugin snapshot (immutable after build)
    snapshot: Option<PluginList>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            snapshot: None,
        }
    }

    /// Register a plugin (build phase)
    pub fn register<P: Plugin>(&mut self, plugin: P) {
        let meta = plugin.meta();
        info!(
            "Registering plugin: {} v{} - {}",
            meta.name, meta.version, meta.description
        );
        self.pending.push(Arc::new(plugin));
    }

    /// Register multiple plugins (supports different plugin types)
    pub fn register_all(&mut self, plugins: impl IntoIterator<Item = PluginBox>) {
        for plugin in plugins {
            let meta = plugin.meta();
            info!(
                "Registering plugin: {} v{} - {}",
                meta.name, meta.version, meta.description
            );
            self.pending.push(Arc::from(plugin));
        }
    }

    /// Build snapshot (plugin list becomes immutable after this)
    pub fn build(&mut self) -> PluginList {
        let snapshot: PluginList = self.pending.drain(..).collect();
        self.snapshot = Some(snapshot.clone());
        snapshot
    }

    /// Get snapshot (if built)
    pub fn snapshot(&self) -> Option<PluginList> {
        self.snapshot.clone()
    }

    /// Get all plugin metadata
    pub fn list(&self) -> Vec<PluginMetadata> {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().map(|p| p.meta()).collect()
        } else {
            self.pending.iter().map(|p| p.meta()).collect()
        }
    }

    /// Get plugin count
    pub fn count(&self) -> usize {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.len()
        } else {
            self.pending.len()
        }
    }

    /// Check if plugin exists by name
    pub fn has(&self, name: &str) -> bool {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().any(|p| p.meta().name == name)
        } else {
            self.pending.iter().any(|p| p.meta().name == name)
        }
    }
}

use std::collections::HashMap;

#[derive(Default, Clone)]
struct TrieNode {
    children: HashMap<char, TrieNode>,
    handlers: Vec<Arc<dyn Plugin>>,
}

impl TrieNode {
    fn insert(&mut self, cmd: &str, plugin: Arc<dyn Plugin>) {
        let mut node = self;
        for c in cmd.chars() {
            node = node.children.entry(c).or_default();
        }
        node.handlers.push(plugin);
    }

    /// Find the longest matching command.
    /// Enforces that the match must be a full word (followed by whitespace or end of string).
    fn match_command(&self, text: &str) -> Option<&Vec<Arc<dyn Plugin>>> {
        let mut node = self;
        let mut last_match = None;
        let mut chars = text.chars().peekable();

        // Check root matches (e.g. empty command? unlikely but possible)
        if !node.handlers.is_empty() {
            // If text is empty or starts with whitespace, root is a match
            if text.is_empty() || text.starts_with(char::is_whitespace) {
                last_match = Some(&node.handlers);
            }
        }

        while let Some(c) = chars.next() {
            match node.children.get(&c) {
                Some(child) => {
                    node = child;
                    // potential match found
                    if !node.handlers.is_empty() {
                        // Check boundary: End of string OR next char is whitespace
                        match chars.peek() {
                            None => last_match = Some(&node.handlers),
                            Some(&next_char) if next_char.is_whitespace() => {
                                last_match = Some(&node.handlers)
                            }
                            _ => {} // Not a boundary, continue
                        }
                    }
                }
                None => break,
            }
        }
        last_match
    }
}

#[derive(Clone)]
pub struct Dispatcher {
    /// Plugins that don't declare specific commands (always checked)
    wildcards: Arc<Vec<Arc<dyn Plugin>>>,
    /// Trie root for command lookup
    root: Arc<TrieNode>,
}

impl Dispatcher {
    /// Create dispatcher from plugin list
    pub fn new(plugins: PluginList) -> Self {
        let mut root = TrieNode::default();
        let mut wildcards = Vec::new();

        for plugin in plugins.iter() {
            let cmds = plugin.commands();
            if cmds.is_empty() {
                wildcards.push(plugin.clone());
            } else {
                for cmd in cmds {
                    root.insert(&cmd, plugin.clone());
                }
            }
        }

        Self {
            wildcards: Arc::new(wildcards),
            root: Arc::new(root),
        }
    }

    /// Dispatch event to all matching plugins
    /// Note: Command-specific plugins are prioritized over wildcards
    pub async fn dispatch(&self, ctx: &Ctx) -> Result<()> {
        let text = ctx.text();
        let mut handled = false;

        // 1. Try to match specific commands using Trie (Longest Prefix Match)
        if let Some(plugins) = self.root.match_command(&text) {
            for plugin in plugins {
                if !plugin.matches(ctx) {
                    continue;
                }

                if let Ok(block) = plugin.handle(ctx).await
                    && block
                {
                    handled = true;
                    break;
                }
            }
        }

        if handled {
            return Ok(());
        }

        // 2. Check wildcards (or if command handlers didn't block)
        for plugin in self.wildcards.iter() {
            if !plugin.matches(ctx) {
                continue;
            }

            if let Ok(block) = plugin.handle(ctx).await
                && block
            {
                break;
            }
        }
        Ok(())
    }
}
