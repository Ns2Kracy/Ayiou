use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

use crate::onebot::ctx::Ctx;

// ============================================================================
// Metadata
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
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Metadata (name/description/version)
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::default()
    }

    /// Check if this plugin matches the context (default: always match)
    fn matches(&self, _ctx: &Ctx) -> bool {
        true
    }

    /// Handle logic, return Ok(true) to block subsequent handlers, Ok(false) to continue
    async fn handle(&self, ctx: &Ctx) -> Result<bool>;
}

pub type PluginBox = Box<dyn Plugin>;

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
                Ok(block) => {
                    if block {
                        break; // Block subsequent handlers
                    }
                }
                Err(_) => {}
            }
        }
        Ok(())
    }
}
