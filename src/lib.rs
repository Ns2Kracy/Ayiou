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
    core::{
        Adapter,
        cron::CronScheduler,
        plugin::{Dispatcher, Plugin, PluginBox, PluginManager},
    },
    onebot::{
        adapter::{OneBotAdapterBuilder, OneBotMessage},
        bot::Bot,
        ctx::Ctx,
    },
};

pub mod core;
pub mod onebot;
pub mod prelude;

pub struct AyiouBot {
    plugin_manager: PluginManager,
    cron_scheduler: Option<CronScheduler>,
    adapter: Option<Box<dyn Adapter<Bot = Bot>>>,
    bot: Option<Bot>,
    event_tx: mpsc::Sender<OneBotMessage>,
    event_rx: mpsc::Receiver<OneBotMessage>,
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
            cron_scheduler: None,
            adapter: None,
            bot: None,
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

    /// Register cron scheduler
    pub fn cron(mut self, scheduler: CronScheduler) -> Self {
        self.cron_scheduler = Some(scheduler);
        self
    }

    /// Register an adapter (e.g., OneBot, Satori)
    ///
    /// Example:
    /// ```ignore
    /// .adapter(OneBotAdapterBuilder::new().ws("ws://..."))
    /// ```
    pub fn adapter(mut self, builder: OneBotAdapterBuilder) -> Self {
        info!("Registering adapter");

        let connection = builder.build(self.event_tx.clone());
        let bot = connection.bot();

        self.adapter = Some(Box::new(connection));
        self.bot = Some(bot);
        self
    }

    /// Start the bot
    pub async fn run(mut self) {
        let Some(bot) = self.bot.clone() else {
            panic!("Bot is not connected, please call .connect() before .run()");
        };

        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", self.plugin_manager.count());

        // Start cron scheduler
        if let Some(scheduler) = self.cron_scheduler {
            scheduler.start(bot.clone());
        }

        // Start adapter (includes driver)
        if let Some(adapter) = self.adapter {
            tokio::spawn(async move {
                if let Err(e) = adapter.run().await {
                    error!("Adapter error: {}", e);
                }
            });
        }

        // Event dispatch task
        let dispatch_bot = bot.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(msg) = self.event_rx.recv().await {
                let OneBotMessage::Event(onebot_event) = msg else {
                    continue;
                };

                let event = Arc::new(onebot_event);
                let dispatcher = dispatcher.clone();
                let bot = dispatch_bot.clone();

                tokio::spawn(async move {
                    let Some(ctx) = Ctx::new(event, bot) else {
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
