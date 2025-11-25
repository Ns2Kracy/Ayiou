use crate::core::{Adapter, Event};
use async_trait::async_trait;
use tracing::{info, warn};

pub mod model;

#[derive(Default)]
pub struct OnebotAdapter;

impl OnebotAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Adapter for OnebotAdapter {
    fn name(&self) -> &'static str {
        "onebot_v11"
    }

    fn parse(&self, raw: &str) -> Option<Event> {
        match serde_json::from_str::<model::OneBotEvent>(raw) {
            Ok(model::OneBotEvent::Message(msg)) => {
                let user_id = msg.user_id.to_string();
                let group_id = msg.group_id.map(|id| id.to_string());
                info!("[{}] {}: {}", self.name(), user_id, msg.raw_message);

                let mut event = Event::new("onebot.message", self.name())
                    .user_id(&user_id)
                    .message(msg.raw_message)
                    .raw(raw);
                if let Some(gid) = group_id {
                    event = event.group_id(gid);
                }
                Some(event)
            }
            Ok(_) => None, // 忽略非消息事件
            Err(err) => {
                warn!("Failed to parse OneBot event: {}", err);
                None
            }
        }
    }
}
