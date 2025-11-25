use std::sync::Arc;

use async_trait::async_trait;
use ayiou::AyiouBot;
use ayiou::adapter::console::ConsoleAdapter;
use ayiou::adapter::onebot_v11::OnebotAdapter;
use ayiou::core::{Ctx, Event, Plugin, PluginMeta};
use ayiou::driver::console::ConsoleDriver;
use ayiou::driver::wsclient::WsClient;
use tracing::info;

struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "ping".to_string(),
            description: "a simple onebot test bot".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    async fn call(&self, event: Arc<Event>, ctx: Arc<Ctx>) -> anyhow::Result<()> {
        let Some(msg) = event.message.as_deref() else {
            return Ok(());
        };

        if msg.trim() == "ping" {
            let target = event
                .group_id
                .as_deref()
                .unwrap_or_else(|| event.user_id.as_deref().unwrap_or("0"));

            // 从哪个平台来的消息，就回复到哪个平台
            if let Some(adapter) = ctx.adapter(&event.platform) {
                info!(
                    "[{}] Ping from {}, replying pong...",
                    event.platform, target
                );
                adapter.send(target, "pong").await?;
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mut bot = AyiouBot::new();

    bot.plugin(MyPlugin);
    bot.register(ConsoleDriver::new(), ConsoleAdapter::new());
    bot.register(
        WsClient::new("ws://10.126.126.1:3001"),
        OnebotAdapter::new(),
    );

    bot.run().await;
}
