use async_trait::async_trait;
use ayiou::adapter::onebot_v11::OneBotAdapter;
use ayiou::prelude::*;
use std::sync::Arc;
use tracing::{Level, info};

// --- A Simple Plugin ---
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
    // Only handle messages
    let Some(msg) = event.message() else { return };

    // Log all messages
    info!(
        "[{}] {}: {}",
        event.platform(),
        event.user_id().unwrap_or("?"),
        msg
    );

    if msg.trim() == "ping"
        && let Some(bot) = ctx.get_any_bot()
    {
        // Reply back
        // Note: OneBot requires a target_id (group_id or user_id)
        // For simplicity, we try to reply to where it came from.

        let target_id = event.group_id().or(event.user_id()).unwrap_or("0");
        let target_type = if event.group_id().is_some() {
            TargetType::Group
        } else {
            TargetType::Private
        };

        info!("Replying PONG to {}", target_id);

        let _ = bot.send_message(target_id, target_type, "pong").await;
    }
}

// --- Main ---

#[tokio::main]
async fn main() {
    // 1. Setup logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting OneBot Example...");

    // 2. Create App with Plugin
    let app = App::new().add_plugin(PingPlugin);

    // 3. Setup OneBot Adapter
    // Change this URL to match your OneBot implementation (e.g., go-cqhttp, NapCat)
    let ws_url = "ws://10.126.126.1:3001";
    let mut adapter = OneBotAdapter::new(ws_url);
    adapter.bind(app.context());

    // 4. Start Adapter in background
    // Note: In a real app, you might want to manage the join handle or use a supervisor.
    tokio::spawn(async move {
        if let Err(e) = adapter.run().await {
            tracing::error!("OneBot Adapter failed: {}", e);
        }
    });

    // 5. Run App (Blocks forever)
    app.run().await;
}
