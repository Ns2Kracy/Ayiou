use std::sync::Arc;

use async_trait::async_trait;
use ayiou::{
    core::{Ctx, Event, Plugin, PluginMeta},
    onebot::model::{Message, MessageEvent},
    AyiouBot,
};

struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "example".to_string(),
            description: "An example plugin for Ayiou.".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    async fn call(&self, event: Arc<Event>, ctx: Arc<Ctx>) -> anyhow::Result<()> {
        let ayiou::onebot::model::OneBotEvent::Message(msg_event) = &event.event else {
            return Ok(());
        };

        let message_content = match &**msg_event {
            MessageEvent::Private(p) => &p.message,

            MessageEvent::Group(g) => &g.message,
        };

        let Message::String(text) = message_content else {
            return Ok(());
        };

        if text.trim() == "ping" {
            let reply = Message::String("pong".to_string());

            match &**msg_event {
                MessageEvent::Private(p) => {
                    ctx.api.send_private_msg(p.user_id, &reply).await?;
                }

                MessageEvent::Group(g) => {
                    ctx.api.send_group_msg(g.group_id, &reply).await?;
                }
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    AyiouBot::new()
        .plugin(MyPlugin)
        .connect("ws://192.168.31.180:3001")
        .run()
        .await;
}
