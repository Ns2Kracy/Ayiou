//! Ayiou: A specialized OneBot v11 bot client.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    connection::BotConnection,
    core::{Ctx, Event, Plugin},
    onebot::{api::Api, model::OneBotEvent},
};

pub mod connection;
pub mod core;
pub mod onebot;

pub struct AyiouBot {
    plugins: Vec<Arc<dyn Plugin>>,
    connection: Option<BotConnection>,
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
            plugins: vec![],
            connection: None,
            api: None,
            event_tx,
            event_rx,
        }
    }

    /// 注册插件
    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        let plugin = Arc::new(plugin);
        info!(
            "Loaded plugin: {}, version: {}",
            plugin.meta().name,
            plugin.meta().version
        );
        self.plugins.push(plugin);
        self
    }

    /// 注册一个 OneBot v11 连接
    pub fn connect(mut self, url: &str) -> Self {
        info!("Connecting to bot at {}", url);
        let (api_tx, api_rx) = mpsc::channel(100);
        let api = Api::new(api_tx);
        let conn = BotConnection::new(url.to_string(), self.event_tx.clone(), api_rx);
        self.connection = Some(conn);
        self.api = Some(api);
        self
    }

    /// 启动 Bot
    pub async fn run(mut self) {
        let plugins = Arc::new(self.plugins);
        let Some(api) = self.api else {
            panic!("Bot is not connected, please call .connect() before .run()");
        };
        let ctx = Arc::new(Ctx::new(api));

        // 事件分发任务
        let dispatch_plugins = plugins.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(onebot_event) = self.event_rx.recv().await {
                let event = Arc::new(Event::new(onebot_event));
                let plugins = dispatch_plugins.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    for plugin in &*plugins {
                        if let Err(err) = plugin.call(event.clone(), ctx.clone()).await {
                            error!("Plugin {} call failed: {}", plugin.meta().name, err)
                        }
                    }
                });
            }
        });

        // 启动连接
        if let Some(conn) = self.connection {
            tokio::spawn(async move {
                if let Err(err) = conn.run().await {
                    error!("Connection error: {}", err);
                }
            });
        }

        info!("Ayiou is running, press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await.unwrap();
        info!("Ayiou is shutting down.");
    }
}
