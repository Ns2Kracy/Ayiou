use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ctx::Ctx, model::OneBotEvent},
    core::{
        app::AppBuilder,
        lifecycle::get_driver,
        plugin::{Dispatcher, Plugin, PluginBox},
    },
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

/// Macro to create a list of plugins, auto-boxed as `PluginBox`
#[macro_export]
macro_rules! plugins {
    ($($plugin:expr),* $(,)?) => {
        vec![$(Box::new($plugin) as $crate::core::plugin::PluginBox),*]
    };
}

/// Ayiou Bot - Main entry point for the bot framework
///
/// Uses the new AppBuilder architecture internally while maintaining
/// backward compatibility with the original API.
pub struct AyiouBot {
    builder: AppBuilder,
    event_tx: mpsc::Sender<OneBotEvent>,
    event_rx: mpsc::Receiver<OneBotEvent>,
}

impl Default for AyiouBot {
    fn default() -> Self {
        Self::new()
    }
}

impl AyiouBot {
    /// Create a new AyiouBot instance
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        Self {
            builder: AppBuilder::new(),
            event_tx,
            event_rx,
        }
    }

    /// Create AyiouBot from an existing AppBuilder
    pub fn from_builder(builder: AppBuilder) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        Self {
            builder,
            event_tx,
            event_rx,
        }
    }

    /// Load configuration from a TOML file
    pub fn config_file(mut self, path: impl AsRef<Path>) -> Result<Self> {
        self.builder = self.builder.config_file(path)?;
        Ok(self)
    }

    /// Register a plugin instance (backward compatible API)
    pub fn register_plugin<P: Plugin>(mut self, plugin: P) -> Self {
        if let Err(e) = self.builder.add_plugin(plugin) {
            error!("Failed to register plugin: {}", e);
        }
        self
    }

    /// Register a plugin by type (requires Default)
    pub fn plugin<P: Plugin + Default>(self) -> Self {
        self.register_plugin(P::default())
    }

    /// Register a command handler (alias for plugin, more semantic for Command enums)
    pub fn command<C: Plugin + Default>(self) -> Self {
        self.plugin::<C>()
    }

    /// Register multiple plugins from boxed trait objects
    pub fn register_plugins(mut self, plugins: impl IntoIterator<Item = PluginBox>) -> Self {
        for plugin in plugins {
            // Convert Box<dyn Plugin> to Arc for AppBuilder
            let arc_plugin: Arc<dyn Plugin> = Arc::from(plugin);
            // We need to register via the internal mechanism
            // For now, we'll use the builder's internal method
            if let Err(e) = self.builder.add_plugin_arc(arc_plugin) {
                error!("Failed to register plugin: {}", e);
            }
        }
        self
    }

    /// Get access to the internal AppBuilder
    pub fn builder(&self) -> &AppBuilder {
        &self.builder
    }

    /// Get mutable access to the internal AppBuilder
    pub fn builder_mut(&mut self) -> &mut AppBuilder {
        &mut self.builder
    }

    /// Start the bot and connect to OneBot via WebSocket
    ///
    /// This method:
    /// 1. Builds the application using AppBuilder (calls lifecycle hooks)
    /// 2. Connects to OneBot WebSocket
    /// 3. Runs the event loop
    /// 4. Calls cleanup on shutdown
    pub async fn run(self, url: impl Into<String>) {
        info!("Building application...");

        // Build the application (triggers build -> ready -> finish lifecycle)
        let mut app = match self.builder.build().await {
            Ok(app) => app,
            Err(e) => {
                error!("Failed to build application: {}", e);
                return;
            }
        };

        // Run startup hooks
        {
            let driver = get_driver().read().await;
            driver.run_startup_hooks().await;
        }

        info!("Connecting to OneBot via WebSocket");

        let outgoing_tx = OneBotV11Adapter::start(url, self.event_tx.clone());

        let plugins = app.plugins().clone();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", app.plugins().len());

        // Event dispatch task
        let mut event_rx = self.event_rx;
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(msg) = event_rx.recv().await {
                let event = Arc::new(msg);
                let dispatcher = dispatcher.clone();
                let outgoing = outgoing_tx.clone();

                tokio::spawn(async move {
                    let Some(ctx) = Ctx::new(event, outgoing) else {
                        return;
                    };

                    if let Err(err) = dispatcher.dispatch(&ctx).await {
                        error!("Plugin dispatch error: {}", err);
                    }
                });
            }
        });

        info!("Ayiou is running, press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await.unwrap();

        // Graceful shutdown
        info!("Ayiou is shutting down...");

        // Run shutdown hooks (before plugin cleanup)
        {
            let driver = get_driver().read().await;
            driver.run_shutdown_hooks().await;
        }

        // Call plugin cleanup hooks
        if let Err(e) = app.shutdown().await {
            error!("Error during shutdown: {}", e);
        }
        info!("Ayiou shutdown complete.");
    }
}
