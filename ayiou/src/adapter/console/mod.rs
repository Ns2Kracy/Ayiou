use crate::core::action::{Bot, TargetType};
use crate::core::event::{BaseEvent, EventKind};
use crate::core::{Adapter, Context, Driver, DriverEvent, Event};
use crate::driver::ConsoleDriver;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ConsoleBot {
    self_id: String,
    driver: Arc<ConsoleDriver>,
}

#[async_trait]
impl Bot for ConsoleBot {
    fn self_id(&self) -> &str {
        &self.self_id
    }

    async fn send_message(
        &self,
        target_id: &str,
        _target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        // In a real impl, we would use self.driver.send()
        // Since ConsoleDriver::send() just prints, we can use it.
        self.driver
            .send(format!(
                "[Bot {} -> {}] {}",
                self.self_id, target_id, content
            ))
            .await?;
        Ok("printed".to_string())
    }
}

pub struct ConsoleAdapter {
    ctx: Option<Context>,
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    fn bind(&mut self, ctx: Context) {
        self.ctx = Some(ctx);
    }

    async fn run(&self) -> Result<()> {
        let ctx = self.ctx.as_ref().expect("Adapter not bound to Context!");

        // Retrieve the event sender from context
        let sender = ctx
            .get::<mpsc::Sender<Arc<dyn Event>>>()
            .expect("Event Sender not found in Context. Is the App initialized correctly?");

        // 1. Create Driver
        let driver = Arc::new(ConsoleDriver);

        // 2. Register Bot
        let bot = Arc::new(ConsoleBot {
            self_id: "console".to_string(),
            driver: driver.clone(),
        });
        ctx.register_bot(bot.clone());

        info!("Console Adapter started via Driver.");

        // 3. Start Driver Loop
        let (tx, mut rx) = mpsc::channel(100);

        let driver_clone = driver.clone();
        tokio::spawn(async move {
            if let Err(e) = driver_clone.start(tx).await {
                tracing::error!("Console Driver failed: {}", e);
            }
        });

        // 4. Listen for Driver Events and convert to App Events
        tokio::spawn(async move {
            while let Some(driver_event) = rx.recv().await {
                if let DriverEvent::Message(text) = driver_event {
                    let event = BaseEvent {
                        platform: "console".to_string(),
                        kind: EventKind::Message,
                        content: text,
                        user_id: "admin".to_string(),
                        group_id: None,
                    };
                    if let Err(e) = sender.send(Arc::new(event)).await {
                        tracing::warn!("Failed to send event to App: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

impl Default for ConsoleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleAdapter {
    pub fn new() -> Self {
        Self { ctx: None }
    }
}
