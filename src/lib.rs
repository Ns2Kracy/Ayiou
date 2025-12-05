use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    connection::BotConnection,
    core::{
        cron::CronScheduler,
        ctx::Ctx,
        plugin::{Dispatcher, Plugin, PluginManager},
    },
    onebot::{api::Api, model::OneBotEvent},
};

pub mod connection;
pub mod core;
pub mod onebot;

pub struct AyiouBot {
    plugin_manager: PluginManager,
    cron_scheduler: Option<CronScheduler>,
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
            plugin_manager: PluginManager::new(),
            cron_scheduler: None,
            connection: None,
            api: None,
            event_tx,
            event_rx,
        }
    }

    /// 注册插件（实现 Plugin trait 即可）
    pub fn plugin<P: Plugin>(mut self, plugin: P) -> Self {
        self.plugin_manager.register(plugin);
        self
    }

    /// 批量注册插件
    pub fn plugins<P: Plugin>(mut self, plugins: impl IntoIterator<Item = P>) -> Self {
        self.plugin_manager.register_all(plugins);
        self
    }

    /// 获取插件管理器
    pub fn plugin_manager(&self) -> &PluginManager {
        &self.plugin_manager
    }

    /// 注册 Cron 定时任务调度器
    pub fn cron(mut self, scheduler: CronScheduler) -> Self {
        self.cron_scheduler = Some(scheduler);
        self
    }

    /// 注册一个 OneBot v11 连接
    pub fn connect(mut self, url: &str) -> Self {
        info!("Connecting to bot at {}", url);
        let (api_tx, api_rx) = mpsc::channel(100);
        let conn = BotConnection::new(url.to_string(), self.event_tx.clone(), api_rx);
        self.connection = Some(conn);
        self.api = Some(Api::new(api_tx));
        self
    }

    /// 启动 Bot
    pub async fn run(mut self) {
        let Some(api) = self.api.clone() else {
            panic!("Bot is not connected, please call .connect() before .run()");
        };

        // 构建插件快照，创建分发器
        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::new(plugins);
        info!("Loaded {} plugins", self.plugin_manager.count());

        // 启动 Cron 调度器
        if let Some(scheduler) = self.cron_scheduler {
            scheduler.start(api.clone());
        }

        // 事件分发任务
        let dispatch_api = api.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(onebot_event) = self.event_rx.recv().await {
                let event = Arc::new(onebot_event);
                let dispatcher = dispatcher.clone();
                let api = dispatch_api.clone();

                // 每个事件独立 spawn，不阻塞接收
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
