use log::{error, info};

use crate::{
    adapter::onebot::v11::adapter::OneBotV11Adapter,
    core::{
        adapter::Adapter,
        plugin::{Dispatcher, Plugin, PluginBox, PluginManager},
    },
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub struct Bot<A: Adapter> {
    plugin_manager: PluginManager<A::Ctx>,
}

impl<A: Adapter> Default for Bot<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Adapter> Bot<A> {
    pub fn new() -> Self {
        Self {
            plugin_manager: PluginManager::new(),
        }
    }

    /// Register a plugin instance
    pub fn register_plugin<P: Plugin<A::Ctx>>(mut self, plugin: P) -> Self {
        self.plugin_manager.register(plugin);
        self
    }

    /// Register a plugin by type (requires Default)
    pub fn plugin<P: Plugin<A::Ctx> + Default>(mut self) -> Self {
        self.plugin_manager.register(P::default());
        self
    }

    /// Register a command handler (alias for plugin, more semantic for Command enums)
    pub fn command<C: Plugin<A::Ctx> + Default>(self) -> Self {
        self.plugin::<C>()
    }

    /// Register multiple plugins
    pub fn register_plugins(
        mut self,
        plugins: impl IntoIterator<Item = PluginBox<A::Ctx>>,
    ) -> Self {
        self.plugin_manager.register_all(plugins);
        self
    }

    /// Get plugin manager
    pub fn plugin_manager(&self) -> &PluginManager<A::Ctx> {
        &self.plugin_manager
    }

    /// Start the bot with a specific adapter
    pub async fn run_adapter(mut self, adapter: A) {
        pretty_env_logger::try_init().ok();
        info!("Starting Bot...");

        let mut event_rx = adapter.start().await;

        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", self.plugin_manager.count());

        // Event dispatch loop directly in main thread or spawn?
        // Original spawned a task. Here we can just run loop.
        // But main.rs calls await on run. So running loop here is fine.

        // Wait, original code spawn a task for dispatch and then waited for ctrl_c.
        // If I run loop here, it blocks.
        // I should probably spawn the loop or run the loop combined with ctrl_c.

        info!("Bot is running, press Ctrl+C to exit.");

        loop {
            tokio::select! {
                Some(ctx) = event_rx.recv() => {
                    let dispatcher = dispatcher.clone();

                    tokio::spawn(async move {
                         // Dispatch takes generic Ctx now.
                        if let Err(err) = dispatcher.dispatch(&ctx).await {
                            error!("Plugin dispatch error: {}", err);
                        }
                    });
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Bot is shutting down.");
                    break;
                }
            }
        }
    }
}

pub type AyiouBot = Bot<OneBotV11Adapter>;

impl AyiouBot {
    /// Start the bot and connect to OneBot via WebSocket
    pub async fn run(self, url: impl Into<String>) {
        let adapter = OneBotV11Adapter::new(url);
        self.run_adapter(adapter).await;
    }
}
