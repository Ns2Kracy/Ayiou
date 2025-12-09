use std::sync::Arc;

/// Macro to create a list of plugins, auto-boxed as `PluginBox`
#[macro_export]
macro_rules! plugins {
    ($($plugin:expr),* $(,)?) => {
        vec![$(Box::new($plugin) as $crate::core::plugin::PluginBox),*]
    };
}

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    adapter::onebot::v11::{adapter::OneBotV11Adapter, api::Api, ctx::Ctx, model::OneBotEvent},
    core::plugin::{Dispatcher, Plugin, PluginBox, PluginManager},
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub struct AyiouBot {
    plugin_manager: PluginManager,
    adapter: Option<OneBotV11Adapter>,
    api: Option<Api>,
    event_tx: mpsc::Sender<OneBotEvent>,
    event_rx: mpsc::Receiver<OneBotEvent>,
}

impl Default for AyiouBot {
    fn default() -> Self {
        Self::new()
    }
}

impl AyiouBot {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        Self {
            plugin_manager: PluginManager::new(),
            adapter: None,
            api: None,
            event_tx,
            event_rx,
        }
    }

    /// Register a plugin
    pub fn register_plugin<P: Plugin>(mut self, plugin: P) -> Self {
        self.plugin_manager.register(plugin);
        self
    }

    /// Register multiple plugins
    pub fn register_plugins(mut self, plugins: impl IntoIterator<Item = PluginBox>) -> Self {
        self.plugin_manager.register_all(plugins);
        self
    }

    /// Get plugin manager
    pub fn plugin_manager(&self) -> &PluginManager {
        &self.plugin_manager
    }

    /// Connect to OneBot via WebSocket
    ///
    /// Example:
    /// ```ignore
    /// AyiouBot::new().connect("ws://127.0.0.1:6700")
    /// ```
    pub fn connect(mut self, url: impl Into<String>) -> Self {
        info!("Connecting to OneBot via WebSocket");
        let mut adapter = OneBotV11Adapter::ws(url);
        let api = adapter.connect(self.event_tx.clone());
        self.adapter = Some(adapter);
        self.api = Some(api);
        self
    }

    /// Start the api
    pub async fn run(mut self) {
        let Some(api) = self.api.clone() else {
            panic!("Bot is not connected, please call .connect() before .run()");
        };

        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", self.plugin_manager.count());

        // Start adapter (includes driver)
        if let Some(adapter) = self.adapter.take() {
            tokio::spawn(async move {
                if let Err(e) = adapter.run().await {
                    error!("Adapter error: {}", e);
                }
            });
        }

        // Event dispatch task
        let dispatch_api = api.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(msg) = self.event_rx.recv().await {
                let event = Arc::new(msg);
                let dispatcher = dispatcher.clone();
                let api = dispatch_api.clone();

                tokio::spawn(async move {
                    let Some(ctx) = Ctx::new(event, api) else {
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
        info!("Ayiou is shutting down.");
    }
}
