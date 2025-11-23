use anyhow::Result;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::warn;

use crate::core::{
    Context, Event, TargetType,
    adapter::Adapter,
    driver::Driver,
    event::{BaseEvent, EventKind},
};

#[derive(Clone)]
pub struct ConsoleAdapter {
    ctx: Context,
    driver: Arc<Mutex<Option<Arc<dyn Driver>>>>,
}

impl ConsoleAdapter {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            driver: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    fn name(&self) -> &'static str {
        "console"
    }

    fn set_driver(&mut self, driver: Arc<dyn Driver>) {
        let mut d = self.driver.lock().unwrap();
        *d = Some(driver);
    }

    async fn handle(&self, raw_event: String) -> Result<()> {
        let sender = self
            .ctx
            .get::<mpsc::Sender<Arc<dyn Event>>>()
            .expect("Event sender not in context!");

        let event = BaseEvent {
            platform: "console".to_string(),
            kind: EventKind::Message,
            content: raw_event,
            user_id: "console_user".to_string(),
            group_id: None,
        };

        if sender.send(Arc::new(event)).await.is_err() {
            warn!("Failed to send console event to app, receiver closed.");
        }

        Ok(())
    }

    fn serialize(
        &self,
        target_id: &str,
        _target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        // For the console, we can just format the output nicely.
        Ok(format!("[Reply -> {}]: {}", target_id, content))
    }

    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        let driver = { self.driver.lock().unwrap().clone() };
        if let Some(driver) = driver {
            let content = self.serialize(target_id, target_type, content)?;
            driver.send(content).await?;
        }
        Ok("".to_string())
    }
}
