use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

mod model;

use crate::{
    adapter::onebot_v11::model::{ApiRequest, OneBotEvent},
    core::{
        adapter::Adapter,
        context::Context,
        driver::Driver,
        event::{BaseEvent, Event, EventKind},
        TargetType,
    },
};

#[derive(Clone)]
pub struct OneBotAdapter {
    ctx: Context,
    driver: Option<Arc<dyn Driver>>,
}

impl OneBotAdapter {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            driver: None,
        }
    }
}

#[async_trait]
impl Adapter for OneBotAdapter {
    fn name(&self) -> &'static str {
        "onebot v11"
    }

    fn set_driver(&mut self, driver: Arc<dyn Driver>) {
        self.driver = Some(driver);
    }

    async fn handle(&self, raw_event: String) -> Result<()> {
        let sender = self
            .ctx
            .get::<mpsc::Sender<Arc<dyn Event>>>()
            .expect("Sender not in context");

        match serde_json::from_str::<OneBotEvent>(&raw_event) {
            Ok(event) => {
                if let OneBotEvent::Message(msg) = event {
                    let core_event = BaseEvent {
                        platform: self.name().to_string(),
                        kind: EventKind::Message,
                        content: msg.raw_message,
                        user_id: msg.user_id.to_string(),
                        group_id: msg.group_id.map(|g| g.to_string()),
                    };
                    if sender.send(Arc::new(core_event)).await.is_err() {
                        warn!("Event channel closed");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse OneBot event: {}", e);
            }
        }
        Ok(())
    }

    fn serialize(&self, target_id: &str, target_type: TargetType, content: &str) -> Result<String> {
        let params = match target_type {
            TargetType::Private => json!({
                "user_id": target_id.parse::<i64>()?,
                "message": content
            }),
            TargetType::Group => json!({
                "group_id": target_id.parse::<i64>()?,
                "message": content
            }),
            _ => return Err(anyhow!("Unsupported target type for OneBot")),
        };

        let req = ApiRequest {
            action: "send_msg".to_string(),
            params,
            echo: None,
        };

        Ok(serde_json::to_string(&req)?)
    }

    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        let driver = self
            .driver
            .as_ref()
            .ok_or_else(|| anyhow!("Adapter is not connected to a driver"))?;
        let raw_msg = self.serialize(target_id, target_type, content)?;
        driver.send(raw_msg).await?;
        Ok("".to_string())
    }
}
