//! Ayiou: A specialized OneBot v11 bot client.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    connection::BotConnection,
    core::{CronScheduler, Ctx, Event, MessageHandler},
    onebot::{api::Api, model::OneBotEvent},
};

pub mod connection;
pub mod core;
pub mod onebot;

pub struct AyiouBot {
    handlers: Vec<MessageHandler>,
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
            handlers: vec![],
            cron_scheduler: None,
            connection: None,
            api: None,
            event_tx,
            event_rx,
        }
    }

    /// 注册消息处理器
    ///
    /// # Example
    /// ```ignore
    /// use ayiou::core::on_command;
    ///
    /// AyiouBot::new()
    ///     .handler(on_command("/ping").name("ping").handle(|ctx| async move {
    ///         ctx.reply_text("pong").await?;
    ///         Ok(false)
    ///     }))
    ///     .connect("ws://...")
    ///     .run()
    ///     .await;
    /// ```
    pub fn handler(mut self, h: MessageHandler) -> Self {
        info!("Loaded handler: {}", h.name);
        self.handlers.push(h);
        self
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
        let api = Api::new(api_tx);
        let conn = BotConnection::new(url.to_string(), self.event_tx.clone(), api_rx);
        self.connection = Some(conn);
        self.api = Some(api);
        self
    }

    /// 启动 Bot
    pub async fn run(mut self) {
        let handlers = Arc::new(self.handlers);
        let Some(api) = self.api.clone() else {
            panic!("Bot is not connected, please call .connect() before .run()");
        };

        // 启动 Cron 调度器
        if let Some(scheduler) = self.cron_scheduler {
            scheduler.start(api.clone());
        }

        // 事件分发任务
        let dispatch_handlers = handlers.clone();
        let dispatch_api = api.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(onebot_event) = self.event_rx.recv().await {
                let event = Arc::new(Event::new(onebot_event));
                let handlers = dispatch_handlers.clone();
                let api = dispatch_api.clone();

                tokio::spawn(async move {
                    // 尝试创建消息上下文
                    let Some(ctx) = Ctx::from_event(event, api) else {
                        return;
                    };

                    for h in &*handlers {
                        if !h.matches(&ctx) {
                            continue;
                        }

                        match h.call(ctx.clone()).await {
                            Ok(block) => {
                                if block {
                                    break; // 返回 true 表示阻止后续处理
                                }
                            }
                            Err(err) => {
                                error!("Handler {} failed: {}", h.name, err);
                            }
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
