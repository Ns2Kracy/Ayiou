use anyhow::Result;
use async_trait::async_trait;
use ayiou::{adapter::onebot_v11::OneBotAdapter, driver::WSClientDriver, prelude::*};
use std::sync::Arc;
use tracing::{info, level_filters::LevelFilter};

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

    let user_id = event.user_id().unwrap_or("?");

    // Log all messages
    info!("[{}] {}: {}", event.platform(), user_id, msg);

    if msg.trim() == "ping" {
        info!("Ping received, attempting to reply...");
        if let Some(bot) = ctx.get_any_bot() {
            let target_id = event
                .group_id()
                .unwrap_or_else(|| event.user_id().unwrap_or("0"));
            let target_type = if event.group_id().is_some() {
                TargetType::Group
            } else {
                TargetType::Private
            };

            info!("Replying PONG to type={:?} id={}", target_type, target_id);

            if let Err(e) = bot.send_message(target_id, target_type, "pong").await {
                tracing::error!("Failed to send message: {}", e);
            }
        } else {
            tracing::warn!("No bot available in context to send reply.");
        }
    }
}

// --- A Generic Bot Implementation ---
// In a real application, this would likely be part of the framework itself.
pub struct GenericBot<A, D> {
    self_id: String,
    adapter: Arc<A>,
    driver: Arc<D>,
}

#[async_trait]
impl<A, D> Bot for GenericBot<A, D>
where
    A: Adapter + Send + Sync,
    D: ayiou::core::Driver + Send + Sync,
{
    fn self_id(&self) -> &str {
        &self.self_id
    }

    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String> {
        let raw_msg = self.adapter.serialize(target_id, target_type, content)?;
        self.driver.send(raw_msg).await?;
        Ok("".to_string())
    }
}

// --- Main ---

#[tokio::main]
async fn main() {
    // 1. Setup logging
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    info!("Starting OneBot Example with new architecture...");

    // 2. Create the AyiouBot instance (the event processor)
    let app = AyiouBot::new().add_plugin(PingPlugin);
    let ctx = app.context();

    // 3. Create the concrete Adapter and Driver
    let adapter = Arc::new(OneBotAdapter::new(ctx.clone()));
    let driver = WSClientDriver::new("ws://192.168.31.180:3001"); // Example URL

    // 4. Compose them into a `Bot` instance that can perform actions
    let bot: Arc<dyn Bot> = Arc::new(GenericBot {
        self_id: "onebot".to_string(), // Example self_id
        adapter: adapter.clone(),
        driver: driver.clone(),
    });

    // 5. Register the bot instance with the context, so plugins can use it
    ctx.register_bot(bot);

    // 6. Spawn the driver to run in the background.
    // The driver will listen for messages and call the adapter's handler.
    let driver_handle = tokio::spawn(async move {
        if let Err(e) = driver.run(adapter).await {
            tracing::error!("Driver failed: {}", e);
        }
    });

    info!("Driver started, starting event loop...");

    // 7. Run the bot's event loop (this blocks)
    let app_handle = tokio::spawn(async move {
        app.run().await;
    });

    // Keep the main function alive
    let _ = tokio::join!(driver_handle, app_handle);
}
