use std::sync::Arc;

/// Macro to create a list of plugins, auto-boxed as `PluginBox`
#[macro_export]
macro_rules! plugins {
    ($($plugin:expr),* $(,)?) => {
        vec![$(Box::new($plugin) as $crate::core::plugin::PluginBox),*]
    };
}

use tokio::sync::mpsc;
use log::{error, info};

use crate::{
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ctx::Ctx, model::OneBotEvent},
    core::plugin::{Dispatcher, Plugin, PluginBox, PluginManager},
    core::session::SessionManager,
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub struct AyiouBot {
    plugin_manager: PluginManager,
    session_manager: Arc<SessionManager>,
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
            session_manager: Arc::new(SessionManager::new()),
            event_tx,
            event_rx,
        }
    }

    /// Register a plugin instance
    pub fn register_plugin<P: Plugin>(mut self, plugin: P) -> Self {
        self.plugin_manager.register(plugin);
        self
    }

    /// Register a plugin by type (requires Default)
    pub fn plugin<P: Plugin + Default>(mut self) -> Self {
        self.plugin_manager.register(P::default());
        self
    }

    /// Register a command handler (alias for plugin, more semantic for Command enums)
    pub fn command<C: Plugin + Default>(self) -> Self {
        self.plugin::<C>()
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

    /// Start the bot and connect to OneBot via WebSocket
    pub async fn run(mut self, url: impl Into<String>) {
        pretty_env_logger::try_init().ok();
        info!("Connecting to OneBot via WebSocket");

        let outgoing_tx = OneBotV11Adapter::start(url, self.event_tx.clone());

        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", self.plugin_manager.count());

        // Event dispatch task
        let mut event_rx = self.event_rx;
        let session_manager = self.session_manager.clone();

        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(msg) = event_rx.recv().await {
                let event = Arc::new(msg);
                let dispatcher = dispatcher.clone();
                let outgoing = outgoing_tx.clone();
                let session_manager = session_manager.clone();

                tokio::spawn(async move {
                    let Some(ctx) = Ctx::new(event, outgoing, session_manager.clone()) else {
                        return;
                    };

                    // Check for active session interception
                    let key = (ctx.user_id(), ctx.group_id());
                    if let Some(tx) = session_manager.get_waiter(&key) {
                        // If we can send to the waiter, we skip normal dispatch
                        // If the receiver dropped (timeout/closed), we fall back to normal dispatch
                        if tx.send(ctx.clone()).await.is_ok() {
                            return;
                        }
                        // Cleanup dead waiter if send failed
                        session_manager.unregister(&key);
                    }

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
