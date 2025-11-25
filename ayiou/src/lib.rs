//! Ayiou Framework
//!
//! An extensible, developer-friendly chat bot framework.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::core::{Adapter, Ctx, Driver, Event, Plugin};

#[forbid(unsafe_code)]
pub mod adapter;
pub mod core;
pub mod driver;

/// Driver + Adapter 组合
struct Connection {
    driver: Arc<dyn Driver>,
    adapter: Arc<dyn Adapter>,
}

pub struct AyiouBot {
    plugins: Vec<Arc<dyn Plugin>>,
    connections: Vec<Connection>,
    event_tx: mpsc::Sender<Event>,
    event_rx: mpsc::Receiver<Event>,
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
            connections: vec![],
            event_tx,
            event_rx,
        }
    }

    pub fn plugin(&mut self, plugin: impl Plugin + 'static) {
        let plugin = Arc::new(plugin);
        info!(
            "Loaded plugin: {}, version: {}",
            plugin.meta().name,
            plugin.meta().version
        );
        self.plugins.push(plugin);
    }

    /// 注册 Driver 和 Adapter
    pub fn register(&mut self, driver: impl Driver + 'static, adapter: impl Adapter + 'static) {
        info!("Registered driver with adapter: {}", adapter.name());
        self.connections.push(Connection {
            driver: Arc::new(driver),
            adapter: Arc::new(adapter),
        });
    }

    pub async fn run(self) {
        let plugins = Arc::new(self.plugins);
        let mut event_rx = self.event_rx;

        // 构建 Ctx，注册所有 Adapter
        let mut ctx = Ctx::new();
        for conn in &self.connections {
            ctx.register_adapter(conn.adapter.clone());
        }
        let ctx = Arc::new(ctx);

        // 事件分发任务
        let dispatch_ctx = ctx.clone();
        tokio::spawn(async move {
            info!("Event dispatch started!");
            while let Some(event) = event_rx.recv().await {
                let plugins = plugins.clone();
                let ctx = dispatch_ctx.clone();
                let event = Arc::new(event);
                tokio::spawn(async move {
                    for plugin in &*plugins {
                        if let Err(err) = plugin.call(event.clone(), ctx.clone()).await {
                            error!("Plugin {} call failed: {}", plugin.meta().name, err)
                        }
                    }
                });
            }
        });

        // 启动每个连接
        for conn in self.connections {
            let event_tx = self.event_tx.clone();
            let (raw_tx, mut raw_rx) = mpsc::channel::<String>(100);
            let (send_tx, mut send_rx) = mpsc::channel::<String>(100);

            // 设置 Adapter 的发送通道
            conn.adapter.set_sender(send_tx);

            // Driver 任务: 接收原始消息 + 发送消息
            let driver = conn.driver.clone();
            let driver_send = conn.driver;
            tokio::spawn(async move {
                if let Err(err) = driver.run(raw_tx).await {
                    error!("Driver error: {}", err);
                }
            });
            tokio::spawn(async move {
                while let Some(msg) = send_rx.recv().await {
                    if let Err(err) = driver_send.send(msg).await {
                        error!("Driver send error: {}", err);
                    }
                }
            });

            // Adapter 解析任务: 转换原始消息为 Event
            let adapter = conn.adapter;
            tokio::spawn(async move {
                while let Some(raw) = raw_rx.recv().await {
                    if let Some(event) = adapter.parse(&raw)
                        && let Err(err) = event_tx.send(event).await
                    {
                        error!("Failed to send event: {}", err);
                    }
                }
            });
        }

        info!("Ayiou is running, press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await.unwrap();
        info!("Ayiou is shutting down.");
    }
}
