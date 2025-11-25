use crate::core::{Adapter, Event};
use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;
use tracing::info;

pub struct ConsoleAdapter {
    tx: OnceCell<mpsc::Sender<String>>,
}

impl Default for ConsoleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleAdapter {
    pub fn new() -> Self {
        Self {
            tx: OnceCell::new(),
        }
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    fn name(&self) -> &'static str {
        "console"
    }

    fn parse(&self, raw: &str) -> Option<Event> {
        info!("[{}] console_user: {}", self.name(), raw);
        Some(
            Event::new("console.message", self.name())
                .user_id("console_user")
                .message(raw)
                .raw(raw),
        )
    }

    fn set_sender(&self, tx: mpsc::Sender<String>) {
        let _ = self.tx.set(tx);
    }

    async fn send(&self, _target: &str, message: &str) -> Result<()> {
        if let Some(tx) = self.tx.get() {
            tx.send(message.to_string()).await?;
        }
        Ok(())
    }
}
