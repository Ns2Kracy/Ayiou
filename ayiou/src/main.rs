use async_trait::async_trait;
use ayiou::adapter::console::ConsoleAdapter;
use ayiou::prelude::*;
use tracing::{Level, info};

// --- User's Plugin Code ---

struct EchoPlugin;

#[async_trait]
impl Plugin for EchoPlugin {
    fn name(&self) -> &'static str {
        "Echo Plugin"
    }

    fn handlers(&self) -> Vec<Box<dyn EventHandler>> {
        // Register the handlers manually for now.
        vec![Box::new(echo_handlerStruct), Box::new(ping_handlerStruct)]
    }
}

// A handler that simply echoes the message
#[handler]
async fn echo_handler(_ctx: Context, event: Arc<dyn Event>) {
    if let Some(msg) = event.message() {
        info!("Echo Plugin Received: {}", msg);
    }
}

// A specific command handler
#[handler]
async fn ping_handler(ctx: Context, event: Arc<dyn Event>) {
    let Some(msg) = event.message() else { return };

    if msg.trim() == "ping" {
        // Try to find a bot to reply with
        if let Some(bot) = ctx.get_any_bot() {
            let _ = bot
                .send_message(
                    "user", // Target ID (in console this is ignored/mocked)
                    TargetType::Private,
                    "PONG! (from Core)",
                )
                .await;
        } else {
            info!("PONG! (No bot found)");
        }
    }
}

// --- Main Entry Point ---

#[tokio::main]
async fn main() {
    // 1. Setup logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // 2. Create App
    let app = App::new().add_plugin(EchoPlugin);

    // 3. Setup Adapter
    let mut console_adapter = ConsoleAdapter::new();
    console_adapter.bind(app.context());
    // Start adapter (this registers the bot)
    let _ = console_adapter.run().await;

    // 4. Run App (Blocks forever)
    app.run().await;
}
