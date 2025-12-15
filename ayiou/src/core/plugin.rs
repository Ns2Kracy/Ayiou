use anyhow::Result;
use std::sync::Arc;
use tracing::info;

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
    pub author: Option<String>,
}

impl PluginMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
            author: None,
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

    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }
}

impl Default for PluginMetadata {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            version: "0.0.0".to_string(),
            author: None,
        }
    }
}

// ============================================================================
// Plugin dependency
// ============================================================================

/// Plugin dependency declaration
#[derive(Clone, Debug)]
pub struct PluginDependency {
    /// Name of the required plugin
    pub name: String,
    /// Whether this dependency is optional
    pub optional: bool,
}

impl PluginDependency {
    /// Create a required dependency
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            optional: false,
        }
    }

    /// Create an optional dependency
    pub fn optional(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            optional: true,
        }
    }
}

// ============================================================================
// Plugin trait with lifecycle
// ============================================================================

/// Plugin trait: main entry point for message handling with lifecycle support
///
/// # Lifecycle
///
/// Plugins go through the following lifecycle stages:
///
/// 1. **Registration**: Plugin is added to AppBuilder
/// 2. **Build** (`build`): Initialize resources, register components
/// 3. **Ready Check** (`ready`): Wait for async initialization to complete
/// 4. **Finish** (`finish`): Post-initialization, all plugins are ready
/// 5. **Running**: Handle events via `matches` and `handle`
/// 6. **Cleanup** (`cleanup`): Graceful shutdown
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// pub struct MyPlugin;
///
/// #[async_trait]
/// impl Plugin for MyPlugin {
///     fn meta(&self) -> PluginMetadata {
///         PluginMetadata::new("my-plugin")
///             .description("My custom plugin")
///             .version("1.0.0")
///     }
///
///     fn dependencies(&self) -> Vec<PluginDependency> {
///         vec![PluginDependency::required("database")]
///     }
///
///     async fn build(&self, app: &mut AppBuilder) -> Result<()> {
///         // Initialize resources
///         Ok(())
///     }
///
///     async fn handle(&self, ctx: &Ctx) -> Result<bool> {
///         // Handle messages
///         Ok(false)
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    // ========== Metadata ==========

    /// Return plugin metadata (name, description, version)
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::default()
    }

    /// Plugin name for unique identification
    ///
    /// Defaults to the type name. Override for custom naming.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// Whether this plugin should be unique (only one instance allowed)
    ///
    /// Defaults to `true`. Set to `false` to allow multiple instances.
    fn is_unique(&self) -> bool {
        true
    }

    // ========== Lifecycle Hooks ==========

    /// Build phase: register resources and configure the application
    ///
    /// Called after all plugins are registered, in dependency order.
    /// Use this to:
    /// - Read configuration
    /// - Initialize connections (database, cache, etc.)
    /// - Register shared resources
    ///
    /// # Arguments
    /// * `app` - Mutable reference to AppBuilder for resource registration
    async fn build(&self, _app: &mut crate::core::app::AppBuilder) -> Result<()> {
        Ok(())
    }

    /// Ready check: return true when the plugin is fully initialized
    ///
    /// Called repeatedly until all plugins return `true`.
    /// Use this for async initialization that may take time.
    ///
    /// # Arguments
    /// * `app` - Reference to the App for checking resources
    fn ready(&self, _app: &crate::core::app::App) -> bool {
        true
    }

    /// Finish phase: called after all plugins are ready
    ///
    /// Use this for initialization that depends on other plugins being ready.
    ///
    /// # Arguments
    /// * `app` - Mutable reference to App
    async fn finish(&self, _app: &mut crate::core::app::App) -> Result<()> {
        Ok(())
    }

    /// Cleanup phase: called during graceful shutdown
    ///
    /// Use this to:
    /// - Close connections
    /// - Flush buffers
    /// - Release resources
    ///
    /// Called in reverse dependency order.
    ///
    /// # Arguments
    /// * `app` - Mutable reference to App
    async fn cleanup(&self, _app: &mut crate::core::app::App) -> Result<()> {
        Ok(())
    }

    // ========== Dependency Management ==========

    /// Declare plugin dependencies
    ///
    /// Dependencies are loaded before this plugin.
    /// Use `PluginDependency::required()` for mandatory dependencies
    /// and `PluginDependency::optional()` for optional ones.
    ///
    /// # Example
    /// ```ignore
    /// fn dependencies(&self) -> Vec<PluginDependency> {
    ///     vec![
    ///         PluginDependency::required("database"),
    ///         PluginDependency::optional("cache"),
    ///     ]
    /// }
    /// ```
    fn dependencies(&self) -> Vec<PluginDependency> {
        vec![]
    }

    // ========== Event Handling ==========

    /// Check if this plugin matches the context (default: always match)
    ///
    /// Return `true` to have `handle` called for this event.
    fn matches(&self, _ctx: &Ctx) -> bool {
        true
    }

    /// Handle the message, return Ok(true) to block subsequent handlers
    ///
    /// This is the main event processing method.
    ///
    /// # Returns
    /// - `Ok(true)` - Event handled, block subsequent plugins
    /// - `Ok(false)` - Event handled, continue to next plugin
    /// - `Err(_)` - Error occurred during handling
    async fn handle(&self, ctx: &Ctx) -> Result<bool>;
}

pub type PluginBox = Box<dyn Plugin>;

pub type PluginList = Arc<[Arc<dyn Plugin>]>;

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

#[derive(Clone)]
pub struct Dispatcher {
    plugins: PluginList,
}

impl Dispatcher {
    /// Create dispatcher from plugin list
    pub fn new(plugins: PluginList) -> Self {
        Self { plugins }
    }

    /// Dispatch event to all matching plugins (sequential, supports blocking)
    pub async fn dispatch(&self, ctx: &Ctx) -> Result<()> {
        for plugin in self.plugins.iter() {
            if !plugin.matches(ctx) {
                continue;
            }

            match plugin.handle(ctx).await {
                Ok(true) => break, // Block subsequent handlers
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.meta().name,
                        error = %e,
                        "Plugin handle error"
                    );
                }
            }
        }
        Ok(())
    }
}
