use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

mod model;

use crate::{
    adapter::onebot_v11::model::{ApiRequest, OneBotEvent},
    core::{
        Adapter, Context, Event,
        action::TargetType,
        event::{BaseEvent, EventKind},
    },
};

#[derive(Clone)]
pub struct OneBotAdapter {
    self_id: String,
    ctx: Option<Context>,
}

impl OneBotAdapter {
    pub fn new(ctx: Context) -> Self {
        Self {
            self_id: "".to_string(),
            ctx: ctx.into(),
        }
    }
}

#[async_trait]
impl Adapter for OneBotAdapter {
    fn name(&self) -> &'static str {
        "onebot v11"
    }

    async fn handle(&self, raw_event: String) -> Result<()> {
        let ctx = self.ctx.as_ref().expect("OneBotAdapter not bound");
        let sender = ctx
            .get::<mpsc::Sender<Arc<dyn Event>>>()
            .expect("Sender not in context");

        match serde_json::from_str::<OneBotEvent>(&raw_event) {
            Ok(event) => {
                match event {
                    OneBotEvent::MetaEvent(_meta) => {} // Ignore meta events for now
                    OneBotEvent::Message(msg) => {
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
        Ok(())
    }

    fn serialize(&self, target_id: &str, target_type: TargetType, content: &str) -> Result<String> {
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

        Ok(serde_json::to_string(&req)?)
    }
}
