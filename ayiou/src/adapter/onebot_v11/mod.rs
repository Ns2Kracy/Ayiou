use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

mod model;

use crate::{
    adapter::onebot_v11::model::{ApiRequest, OneBotEvent},
    core::{
        Adapter, Bot, Context, Driver, DriverEvent, Event, TargetType,
        event::{BaseEvent, EventKind},
    },
    driver::WSClientDriver,
};

#[derive(Debug, Clone)]
pub struct OneBotBot {
    self_id: String,
    driver: Arc<WSClientDriver>,
}

#[async_trait]
impl Bot for OneBotBot {
    fn self_id(&self) -> &str {
        &self.self_id
    }

    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        let action = "send_msg";
        let params = match target_type {
            TargetType::Private => json!({
                "user_id": target_id.parse::<i64>()?,
                "message": content
            }),
            TargetType::Group => json!({
                "group_id": target_id.parse::<i64>()?,
                "message": content
            }),
            _ => return Err(anyhow::anyhow!("Unsupported target type for OneBot")),
        };

        let req = ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        };

        self.driver.send(serde_json::to_string(&req)?).await?;
        // Note: Real implementation needs to wait for response with echo matching.
        // Here we just fire and forget for simplicity.
        Ok("".to_string())
    }
}

pub struct OneBotAdapter {
    ws_url: String,
    ctx: Option<Context>,
}

impl OneBotAdapter {
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            ctx: None,
        }
    }
}

#[async_trait]
impl Adapter for OneBotAdapter {
    fn bind(&mut self, ctx: Context) {
        self.ctx = Some(ctx);
    }

    async fn run(&self) -> Result<()> {
        let ctx = self.ctx.as_ref().expect("OneBotAdapter not bound");
        let sender = ctx
            .get::<mpsc::Sender<Arc<dyn Event>>>()
            .expect("Sender not in context");

        let driver = WSClientDriver::new(&self.ws_url);
        let (tx, mut rx) = mpsc::channel(100);

        // Start driver loop
        let driver_clone = driver.clone();
        tokio::spawn(async move {
            if let Err(e) = driver_clone.start(tx).await {
                error!("OneBot WS Driver error: {}", e);
            }
        });

        info!("OneBot Adapter connected to {}", self.ws_url);

        // Handle events
        while let Some(driver_event) = rx.recv().await {
            if let DriverEvent::Message(text) = driver_event {
                // Parse OneBot JSON
                match serde_json::from_str::<OneBotEvent>(&text) {
                    Ok(event) => {
                        match event {
                            OneBotEvent::MetaEvent(meta) => {
                                // Heartbeat usually contains self_id. We can register bot here if not registered.
                                if meta.meta_event_type == "lifecycle"
                                    || meta.meta_event_type == "heartbeat"
                                {
                                    // Register Bot if new
                                    let self_id = meta.self_id.to_string();
                                    if ctx.get_bot(&self_id).is_none() {
                                        let bot = Arc::new(OneBotBot {
                                            self_id: self_id.clone(),
                                            driver: driver.clone(),
                                        });
                                        ctx.register_bot(bot);
                                        info!("Registered OneBot: {}", self_id);
                                    }
                                }
                            }
                            OneBotEvent::Message(msg) => {
                                // Convert to Core Event
                                let core_event = BaseEvent {
                                    platform: "onebot".to_string(),
                                    kind: EventKind::Message,
                                    content: msg.raw_message,
                                    user_id: msg.user_id.to_string(),
                                    group_id: msg.group_id.map(|g| g.to_string()),
                                };
                                let _ = sender.send(Arc::new(core_event)).await;
                            }
                            _ => {} // Ignore others
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse OneBot event: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
