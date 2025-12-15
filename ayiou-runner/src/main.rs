//! Ayiou Runner - Complete bot runner with external plugin support
//!
//! This binary provides a ready-to-use bot that:
//! - Loads configuration from `config.toml`
//! - Connects to OneBot via WebSocket
//! - Supports external plugins via `ExternalPluginBridge`

use anyhow::Result;
use ayiou::prelude::*;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Ayiou Bot...");

    use ayiou_plugin_bridge::ExternalPluginBridge;

    let mut builder = AppBuilder::new();

    // Load config file if exists
    if std::path::Path::new("config.toml").exists() {
        info!("Loading config.toml");
        builder = builder.config_file("config.toml")?;
    }

    // Get bot configuration (ws_url with access_token)
    let bot_config: BotConfig = builder.config()?;
    let ws_url = bot_config.ws_url_with_token();

    info!("OneBot WebSocket URL: {}", bot_config.ws_url);
    if bot_config.access_token.is_some() {
        info!("Access token configured");
    }

    // Add external plugin bridge
    builder.add_plugin(ExternalPluginBridge::new())?;

    // Create AyiouBot from builder and run
    // This will:
    // 1. Build the app (trigger lifecycle hooks)
    // 2. Connect to OneBot WebSocket
    // 3. Run the event loop
    // 4. Handle graceful shutdown
    AyiouBot::from_builder(builder).run(ws_url).await;

    Ok(())
}
