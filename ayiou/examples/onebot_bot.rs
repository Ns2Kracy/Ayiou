use anyhow::Result;
use async_trait::async_trait;
use ayiou::{
    adapter::onebot_v11::OneBotAdapter,
    bot::AyiouBot,
    core::{Context, Event, Plugin, TargetType, event::EventHandler},
    driver::WSClientDriver,
};
use ayiou_macros::handler;
use std::sync::Arc;
use tracing::{info, level_filters::LevelFilter};

struct PingPlugin;

#[async_trait]
impl Plugin for PingPlugin {
    fn name(&self) -> &'static str {
        "Ping Plugin"
    }

    fn handlers(&self) -> Vec<Box<dyn EventHandler>> {
        vec![Box::new(ping_handlerStruct)]
    }
}

#[handler]
async fn ping_handler(ctx: Context, event: Arc<dyn Event>) {
    let Some(msg) = event.message() else { return };
    let user_id = event.user_id().unwrap_or("?");
    info!("[{}] {}: {}", event.platform(), user_id, msg);

    if msg.trim() == "ping" {
        info!("Ping received, attempting to reply...");
        if let Some(adapter) = ctx.get_any_adapter() {
            let target_id = event
                .group_id()
                .unwrap_or_else(|| event.user_id().unwrap_or("0"));
            let target_type = if event.group_id().is_some() {
                TargetType::Group
            } else {
                TargetType::Private
            };

            info!("Replying PONG to type={:?} id={}", target_type, target_id);

            if let Err(e) = adapter.send_message(target_id, target_type, "pong").await {
                tracing::error!("Failed to send message: {}", e);
            }
        } else {
            tracing::warn!("No adapter available in context to send reply.");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    info!("Starting OneBot Example...");
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    AyiouBot::new()
        .plugin(PingPlugin)
        .register_adapter(OneBotAdapter::new)
        .register_driver(WSClientDriver::new("ws://192.168.31.180:3001"))
        .run()
        .await?;

    Ok(())
}
