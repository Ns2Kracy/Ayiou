use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use crate::core::{
    Adapter, Context, Event,
    action::TargetType,
    event::{BaseEvent, EventKind},
};

#[derive(Clone)]
pub struct ConsoleAdapter {
    ctx: Option<Context>,
}

impl ConsoleAdapter {
    pub fn new(ctx: Context) -> Self {
        Self { ctx: Some(ctx) }
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    fn name(&self) -> &'static str {
        "console"
    }

    async fn handle(&self, raw_event: String) -> Result<()> {
        let ctx = self.ctx.as_ref().expect("Adapter not bound to a context!");
        let sender = ctx
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
}
