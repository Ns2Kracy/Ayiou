//! Dynamic plugin management with lifecycle, hot-reload, and WASM support.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::adapter::onebot::v11::ctx::Ctx;
use crate::core::plugin::{Plugin, PluginMetadata};

// ============================================================================
// Plugin State
// ============================================================================

/// Plugin lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Plugin is loaded but not enabled
    Loaded,
    /// Plugin is enabled and processing events
    Enabled,
    /// Plugin is disabled (won't process events)
    Disabled,
    /// Plugin encountered an error
    Error,
}

/// Plugin source type
#[derive(Debug, Clone)]
pub enum PluginSource {
    /// Compiled into the binary (static)
    Static,
    /// Loaded from WASM file
    Wasm(PathBuf),
    /// Downloaded from remote URL
    Remote { url: String, local_path: PathBuf },
}

// ============================================================================
// Plugin Entry
// ============================================================================

/// A managed plugin entry with state and metadata
pub struct PluginEntry {
    /// Unique plugin ID
    pub id: String,
    /// Plugin instance
    pub plugin: Arc<dyn Plugin>,
    /// Current state
    pub state: PluginState,
    /// Source information
    pub source: PluginSource,
    /// Saved state for hot-reload
    pub saved_state: Option<serde_json::Value>,
}

impl PluginEntry {
    pub fn new(plugin: Arc<dyn Plugin>, source: PluginSource) -> Self {
        let meta = plugin.meta();
        Self {
            id: meta.name.clone(),
            plugin,
            state: PluginState::Loaded,
            source,
            saved_state: None,
        }
    }

    pub fn meta(&self) -> PluginMetadata {
        self.plugin.meta()
    }

    pub fn is_enabled(&self) -> bool {
        self.state == PluginState::Enabled
    }
}

// ============================================================================
// Dynamic Plugin Registry
// ============================================================================

/// Thread-safe dynamic plugin registry supporting runtime modifications
pub struct DynamicPluginRegistry {
    /// All registered plugins (id -> entry)
    plugins: RwLock<HashMap<String, PluginEntry>>,
    /// Plugin load order for dispatch priority
    load_order: RwLock<Vec<String>>,
    /// Plugin directory for WASM plugins
    plugin_dir: PathBuf,
}

impl DynamicPluginRegistry {
    pub fn new(plugin_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            load_order: RwLock::new(Vec::new()),
            plugin_dir: plugin_dir.into(),
        }
    }

    /// Register a static (compiled) plugin
    pub async fn register_static<P: Plugin>(&self, plugin: P) -> Result<()> {
        let arc_plugin: Arc<dyn Plugin> = Arc::new(plugin);
        let meta = arc_plugin.meta();
        let id = meta.name.clone();

        // Call on_load lifecycle hook
        arc_plugin
            .on_load()
            .await
            .context("Plugin on_load failed")?;

        let entry = PluginEntry::new(arc_plugin, PluginSource::Static);

        {
            let mut plugins = self.plugins.write();
            let mut order = self.load_order.write();

            if plugins.contains_key(&id) {
                anyhow::bail!("Plugin '{}' already registered", id);
            }

            info!("Registered static plugin: {} v{}", meta.name, meta.version);
            plugins.insert(id.clone(), entry);
            order.push(id);
        }

        Ok(())
    }

    /// Enable a plugin by ID
    pub async fn enable(&self, id: &str) -> Result<()> {
        let plugin = {
            let plugins = self.plugins.read();
            plugins
                .get(id)
                .map(|e| e.plugin.clone())
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?
        };

        plugin
            .on_enable()
            .await
            .context("Plugin on_enable failed")?;

        {
            let mut plugins = self.plugins.write();
            if let Some(entry) = plugins.get_mut(id) {
                entry.state = PluginState::Enabled;
                info!("Enabled plugin: {}", id);
            }
        }

        Ok(())
    }

    /// Disable a plugin by ID
    pub async fn disable(&self, id: &str) -> Result<()> {
        let plugin = {
            let plugins = self.plugins.read();
            plugins
                .get(id)
                .map(|e| e.plugin.clone())
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?
        };

        plugin
            .on_disable()
            .await
            .context("Plugin on_disable failed")?;

        {
            let mut plugins = self.plugins.write();
            if let Some(entry) = plugins.get_mut(id) {
                entry.state = PluginState::Disabled;
                info!("Disabled plugin: {}", id);
            }
        }

        Ok(())
    }

    /// Unload a plugin by ID
    pub async fn unload(&self, id: &str) -> Result<()> {
        let plugin = {
            let mut plugins = self.plugins.write();
            plugins
                .remove(id)
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?
        };

        // Remove from load order
        {
            let mut order = self.load_order.write();
            order.retain(|x| x != id);
        }

        plugin
            .plugin
            .on_unload()
            .await
            .context("Plugin on_unload failed")?;

        info!("Unloaded plugin: {}", id);
        Ok(())
    }

    /// Hot-reload a WASM plugin
    pub async fn reload(&self, id: &str) -> Result<()> {
        let (plugin, source) = {
            let plugins = self.plugins.read();
            let entry = plugins
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;

            match &entry.source {
                PluginSource::Wasm(path) => (entry.plugin.clone(), path.clone()),
                PluginSource::Remote { local_path, .. } => {
                    (entry.plugin.clone(), local_path.clone())
                }
                PluginSource::Static => {
                    anyhow::bail!("Cannot hot-reload static plugins");
                }
            }
        };

        // Save state before reload
        let saved_state = plugin
            .on_before_reload()
            .await
            .context("Plugin on_before_reload failed")?;

        info!("Reloading plugin: {} from {:?}", id, source);

        // Load new WASM module
        let new_plugin = self.load_wasm_plugin(&source).await?;

        // Call lifecycle hooks
        new_plugin
            .on_load()
            .await
            .context("New plugin on_load failed")?;
        new_plugin
            .on_after_reload(saved_state)
            .await
            .context("New plugin on_after_reload failed")?;

        // Replace plugin instance
        {
            let mut plugins = self.plugins.write();
            if let Some(entry) = plugins.get_mut(id) {
                entry.plugin = new_plugin;
                entry.state = PluginState::Enabled;
            }
        }

        info!("Plugin {} reloaded successfully", id);
        Ok(())
    }

    /// Load a WASM plugin from file
    #[cfg(feature = "wasm")]
    async fn load_wasm_plugin(&self, path: &Path) -> Result<Arc<dyn Plugin>> {
        let runtime = super::wasm::WasmRuntime::new()?;
        let plugin = runtime.load_plugin(path).await?;
        Ok(Arc::new(plugin))
    }

    /// Load a WASM plugin from file (stub when wasm feature disabled)
    #[cfg(not(feature = "wasm"))]
    async fn load_wasm_plugin(&self, _path: &Path) -> Result<Arc<dyn Plugin>> {
        anyhow::bail!("WASM support not enabled - compile with 'wasm' feature")
    }

    /// Get all enabled plugins in load order
    pub fn enabled_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.plugins.read();
        let order = self.load_order.read();

        order
            .iter()
            .filter_map(|id| {
                plugins.get(id).and_then(|e| {
                    if e.is_enabled() {
                        Some(e.plugin.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// List all plugins with their states
    pub fn list(&self) -> Vec<(PluginMetadata, PluginState)> {
        let plugins = self.plugins.read();
        let order = self.load_order.read();

        order
            .iter()
            .filter_map(|id| plugins.get(id).map(|e| (e.meta(), e.state)))
            .collect()
    }

    /// Get plugin count
    pub fn count(&self) -> usize {
        self.plugins.read().len()
    }

    /// Check if plugin exists
    pub fn has(&self, id: &str) -> bool {
        self.plugins.read().contains_key(id)
    }

    /// Get plugin state
    pub fn state(&self, id: &str) -> Option<PluginState> {
        self.plugins.read().get(id).map(|e| e.state)
    }

    /// Get plugin directory
    pub fn plugin_dir(&self) -> &Path {
        &self.plugin_dir
    }
}

// ============================================================================
// Dynamic Dispatcher
// ============================================================================

/// Dispatcher that works with DynamicPluginRegistry
#[derive(Clone)]
pub struct DynamicDispatcher {
    registry: Arc<DynamicPluginRegistry>,
}

impl DynamicDispatcher {
    pub fn new(registry: Arc<DynamicPluginRegistry>) -> Self {
        Self { registry }
    }

    /// Dispatch event to all enabled plugins
    pub async fn dispatch(&self, ctx: &Ctx) -> Result<()> {
        let plugins = self.registry.enabled_plugins();

        for plugin in plugins {
            let meta = plugin.meta();
            let matched = plugin.matches(ctx);
            tracing::debug!("Plugin '{}' matches: {}", meta.name, matched);

            if !matched {
                continue;
            }

            tracing::debug!("Plugin '{}' handling message", meta.name);
            match plugin.handle(ctx).await {
                Ok(true) => {
                    tracing::debug!("Plugin '{}' blocked further handlers", meta.name);
                    break;
                }
                Ok(false) => continue,
                Err(e) => {
                    error!("Plugin {} error: {}", meta.name, e);
                    continue;
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Plugin Manager Commands (for runtime control)
// ============================================================================

/// Commands for controlling plugins at runtime
#[derive(Debug, Clone)]
pub enum PluginCommand {
    /// Enable a plugin
    Enable(String),
    /// Disable a plugin
    Disable(String),
    /// Reload a WASM plugin
    Reload(String),
    /// Unload a plugin
    Unload(String),
    /// Load a WASM plugin from path
    LoadWasm(PathBuf),
    /// Download and load a remote plugin
    LoadRemote { url: String, name: String },
    /// List all plugins
    List,
}

/// Plugin manager that handles commands asynchronously
pub struct PluginCommandHandler {
    registry: Arc<DynamicPluginRegistry>,
    cmd_rx: mpsc::Receiver<PluginCommand>,
}

impl PluginCommandHandler {
    pub fn new(registry: Arc<DynamicPluginRegistry>) -> (Self, mpsc::Sender<PluginCommand>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        (Self { registry, cmd_rx }, cmd_tx)
    }

    /// Run the command handler loop
    pub async fn run(mut self) {
        while let Some(cmd) = self.cmd_rx.recv().await {
            if let Err(e) = self.handle_command(cmd).await {
                error!("Plugin command error: {}", e);
            }
        }
    }

    async fn handle_command(&self, cmd: PluginCommand) -> Result<()> {
        match cmd {
            PluginCommand::Enable(id) => {
                self.registry.enable(&id).await?;
            }
            PluginCommand::Disable(id) => {
                self.registry.disable(&id).await?;
            }
            PluginCommand::Reload(id) => {
                self.registry.reload(&id).await?;
            }
            PluginCommand::Unload(id) => {
                self.registry.unload(&id).await?;
            }
            PluginCommand::LoadWasm(path) => {
                info!("Loading WASM plugin from {:?}", path);
                // Will be implemented with wasm module
                warn!("WASM loading not yet implemented");
            }
            PluginCommand::LoadRemote { url, name } => {
                info!("Downloading plugin {} from {}", name, url);
                // Will be implemented with remote loader
                warn!("Remote loading not yet implemented");
            }
            PluginCommand::List => {
                let plugins = self.registry.list();
                info!("Registered plugins:");
                for (meta, state) in plugins {
                    info!("  - {} v{} [{:?}]", meta.name, meta.version, state);
                }
            }
        }
        Ok(())
    }
}
