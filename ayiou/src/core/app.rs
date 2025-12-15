//! Application builder and runtime for the plugin system.
//!
//! This module provides the core infrastructure for building and running
//! applications with the new plugin lifecycle system.

use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tracing::{debug, info, warn};

use crate::core::config::{ConfigStore, Configurable};
use crate::core::plugin::{Plugin, PluginList};

// ============================================================================
// Resource Registry
// ============================================================================

/// Type-erased resource storage for dependency injection
pub struct ResourceRegistry {
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceRegistry {
    /// Create a new empty resource registry
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Insert a resource into the registry
    ///
    /// If a resource of the same type already exists, it will be replaced.
    pub fn insert<R: Send + Sync + 'static>(&mut self, resource: R) {
        let type_id = TypeId::of::<R>();
        debug!("Inserting resource: {}", std::any::type_name::<R>());
        self.resources.insert(type_id, Box::new(resource));
    }

    /// Get a reference to a resource
    pub fn get<R: 'static>(&self) -> Option<&R> {
        let type_id = TypeId::of::<R>();
        self.resources
            .get(&type_id)
            .and_then(|r| r.downcast_ref::<R>())
    }

    /// Get a mutable reference to a resource
    pub fn get_mut<R: 'static>(&mut self) -> Option<&mut R> {
        let type_id = TypeId::of::<R>();
        self.resources
            .get_mut(&type_id)
            .and_then(|r| r.downcast_mut::<R>())
    }

    /// Check if a resource exists
    pub fn contains<R: 'static>(&self) -> bool {
        let type_id = TypeId::of::<R>();
        self.resources.contains_key(&type_id)
    }

    /// Remove a resource from the registry
    pub fn remove<R: 'static>(&mut self) -> Option<R> {
        let type_id = TypeId::of::<R>();
        self.resources
            .remove(&type_id)
            .and_then(|r| r.downcast::<R>().ok())
            .map(|r| *r)
    }
}

// ============================================================================
// Application State
// ============================================================================

/// Application lifecycle state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppState {
    /// Application is being built
    Building,
    /// Application is running
    Running,
    /// Application is shutting down
    ShuttingDown,
}

// ============================================================================
// Application Builder
// ============================================================================

/// Builder for constructing an application with plugins
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// let app = AppBuilder::new()
///     .config_file("config.toml")?
///     .add_plugin(MyPlugin)?
///     .build()
///     .await?;
/// ```
pub struct AppBuilder {
    config: ConfigStore,
    resources: ResourceRegistry,
    plugins: Vec<Arc<dyn Plugin>>,
    plugin_names: HashSet<String>,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Create a new application builder
    pub fn new() -> Self {
        Self {
            config: ConfigStore::empty(),
            resources: ResourceRegistry::new(),
            plugins: Vec::new(),
            plugin_names: HashSet::new(),
        }
    }

    /// Load configuration from a file
    pub fn config_file(mut self, path: impl AsRef<Path>) -> Result<Self> {
        self.config = ConfigStore::from_file(path)?;
        Ok(self)
    }

    /// Set configuration store directly
    pub fn config_store(mut self, config: ConfigStore) -> Self {
        self.config = config;
        self
    }

    /// Register a plugin
    ///
    /// # Errors
    ///
    /// Returns an error if a unique plugin with the same name is already registered.
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> Result<&mut Self> {
        let name = plugin.meta().name.clone();

        // Check uniqueness
        if plugin.is_unique() && self.plugin_names.contains(&name) {
            return Err(anyhow!("Plugin '{}' is already registered", name));
        }

        info!(
            "Adding plugin: {} v{} - {}",
            plugin.meta().name,
            plugin.meta().version,
            plugin.meta().description
        );

        self.plugin_names.insert(name);
        self.plugins.push(Arc::new(plugin));
        Ok(self)
    }

    /// Register multiple plugins from a plugin group
    pub fn add_plugins<G: PluginGroup>(&mut self, group: G) -> Result<&mut Self> {
        group.build(self)?;
        Ok(self)
    }

    /// Register a plugin from an Arc (for use with boxed trait objects)
    ///
    /// This is primarily used internally when converting from `Box<dyn Plugin>`.
    pub fn add_plugin_arc(&mut self, plugin: Arc<dyn Plugin>) -> Result<&mut Self> {
        let name = plugin.meta().name.clone();

        // Check uniqueness
        if plugin.is_unique() && self.plugin_names.contains(&name) {
            return Err(anyhow!("Plugin '{}' is already registered", name));
        }

        info!(
            "Adding plugin: {} v{} - {}",
            plugin.meta().name,
            plugin.meta().version,
            plugin.meta().description
        );

        self.plugin_names.insert(name);
        self.plugins.push(plugin);
        Ok(self)
    }

    /// Get a configuration value
    pub fn config<C: Configurable>(&self) -> Result<C> {
        self.config.get::<C>()
    }

    /// Insert a resource
    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, resource: R) -> &mut Self {
        self.resources.insert(resource);
        self
    }

    /// Get a resource reference
    pub fn get_resource<R: 'static>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    /// Get a mutable resource reference
    pub fn get_resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    /// Build the application
    ///
    /// This will:
    /// 1. Sort plugins by dependencies (topological sort)
    /// 2. Call `build()` on each plugin in order
    /// 3. Wait for all plugins to be ready
    /// 4. Call `finish()` on each plugin
    pub async fn build(mut self) -> Result<App> {
        info!("Building application with {} plugins", self.plugins.len());

        // 1. Sort plugins by dependencies
        let sorted_plugins = self.sort_by_dependencies()?;
        info!("Plugin load order determined");

        // 2. Call build() on each plugin
        for plugin in &sorted_plugins {
            debug!("Building plugin: {}", plugin.meta().name);
            plugin.build(&mut self).await?;
        }

        // 3. Create App
        let mut app = App {
            config: self.config,
            resources: self.resources,
            plugins: sorted_plugins.into(),
            state: AppState::Building,
        };

        // 4. Wait for all plugins to be ready
        app.wait_ready().await;

        // 5. Call finish() on each plugin
        let plugins = app.plugins.clone();
        for plugin in plugins.iter() {
            debug!("Finishing plugin: {}", plugin.meta().name);
            plugin.finish(&mut app).await?;
        }

        app.state = AppState::Running;
        info!("Application built successfully");

        Ok(app)
    }

    /// Sort plugins by dependencies using Kahn's algorithm (topological sort)
    fn sort_by_dependencies(&self) -> Result<Vec<Arc<dyn Plugin>>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize in-degree for all plugins
        for plugin in &self.plugins {
            let name = plugin.meta().name.clone();
            in_degree.entry(name.clone()).or_insert(0);

            for dep in plugin.dependencies() {
                // Check if required dependency exists
                if !dep.optional && !self.plugin_names.contains(&dep.name) {
                    return Err(anyhow!(
                        "Plugin '{}' requires '{}' which is not registered",
                        name,
                        dep.name
                    ));
                }

                // Only add edge if dependency exists
                if self.plugin_names.contains(&dep.name) {
                    graph.entry(dep.name.clone()).or_default().push(name.clone());
                    *in_degree.entry(name.clone()).or_insert(0) += 1;
                }
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut sorted = Vec::new();

        while let Some(name) = queue.pop_front() {
            // Find the plugin with this name
            let plugin = self
                .plugins
                .iter()
                .find(|p| p.meta().name == name)
                .cloned()
                .unwrap();
            sorted.push(plugin);

            // Update in-degrees
            if let Some(dependents) = graph.get(&name) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        // Check for circular dependencies
        if sorted.len() != self.plugins.len() {
            let remaining: Vec<_> = self
                .plugins
                .iter()
                .filter(|p| !sorted.iter().any(|s| s.meta().name == p.meta().name))
                .map(|p| p.meta().name.clone())
                .collect();
            return Err(anyhow!(
                "Circular dependency detected among plugins: {:?}",
                remaining
            ));
        }

        Ok(sorted)
    }
}

// ============================================================================
// Plugin Group
// ============================================================================

/// Trait for grouping multiple plugins together
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// pub struct CorePlugins;
///
/// impl PluginGroup for CorePlugins {
///     fn build(self, app: &mut AppBuilder) -> Result<()> {
///         app.add_plugin(LogPlugin)?;
///         app.add_plugin(ConfigPlugin)?;
///         Ok(())
///     }
/// }
/// ```
pub trait PluginGroup {
    /// Add all plugins in this group to the application builder
    fn build(self, app: &mut AppBuilder) -> Result<()>;
}

/// Macro for easily defining plugin groups
#[macro_export]
macro_rules! plugin_group {
    ($name:ident { $($plugin:expr),* $(,)? }) => {
        pub struct $name;

        impl $crate::core::app::PluginGroup for $name {
            fn build(self, app: &mut $crate::core::app::AppBuilder) -> anyhow::Result<()> {
                $(app.add_plugin($plugin)?;)*
                Ok(())
            }
        }
    };
}

// ============================================================================
// Application Runtime
// ============================================================================

/// The running application
///
/// Holds all resources, configuration, and plugins for the application.
pub struct App {
    config: ConfigStore,
    resources: ResourceRegistry,
    plugins: PluginList,
    state: AppState,
}

impl App {
    /// Get the current application state
    pub fn state(&self) -> AppState {
        self.state
    }

    /// Get a configuration value
    pub fn config<C: Configurable>(&self) -> Result<C> {
        self.config.get::<C>()
    }

    /// Reload configuration from file
    pub fn reload_config(&mut self) -> Result<()> {
        self.config.reload()
    }

    /// Get a resource reference
    pub fn get_resource<R: 'static>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    /// Get a mutable resource reference
    pub fn get_resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    /// Insert a resource
    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, resource: R) {
        self.resources.insert(resource);
    }

    /// Get the plugin list
    pub fn plugins(&self) -> &PluginList {
        &self.plugins
    }

    /// Wait for all plugins to be ready
    async fn wait_ready(&self) {
        let max_wait = Duration::from_secs(30);
        let check_interval = Duration::from_millis(100);
        let start = std::time::Instant::now();

        loop {
            let all_ready = self.plugins.iter().all(|p| p.ready(self));
            if all_ready {
                info!("All plugins are ready");
                return;
            }

            if start.elapsed() > max_wait {
                warn!("Timeout waiting for plugins to be ready, continuing anyway");
                return;
            }

            tokio::time::sleep(check_interval).await;
        }
    }

    /// Shutdown the application gracefully
    ///
    /// Calls `cleanup()` on all plugins in reverse order.
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down application");
        self.state = AppState::ShuttingDown;

        // Call cleanup in reverse order
        let plugins = self.plugins.clone();
        for plugin in plugins.iter().rev() {
            debug!("Cleaning up plugin: {}", plugin.meta().name);
            if let Err(e) = plugin.cleanup(self).await {
                warn!("Error cleaning up plugin {}: {}", plugin.meta().name, e);
            }
        }

        info!("Application shutdown complete");
        Ok(())
    }
}
