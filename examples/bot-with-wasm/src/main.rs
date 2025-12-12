//! Ayiou Bot with WASM Plugin Loading Example
//!
//! This example demonstrates how to:
//! 1. Create a bot with static plugins
//! 2. Load WASM plugins at runtime
//! 3. Use the dynamic plugin registry

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use ayiou::prelude::*;
use tracing::{info, Level};

// ============================================================================
// Static Plugin Example (compiled into binary)
// ============================================================================

#[derive(Args)]
pub struct Ping;

impl Ping {
    pub async fn handle(&self, ctx: &Ctx) -> anyhow::Result<()> {
        ctx.reply_text("pong! 🏓").await?;
        Ok(())
    }
}

#[derive(Args)]
pub struct Status;

impl Status {
    pub async fn handle(&self, ctx: &Ctx) -> anyhow::Result<()> {
        ctx.reply_text("Bot is running with WASM plugin support! ✅").await?;
        Ok(())
    }
}

#[derive(Plugin)]
#[plugin(name = "basic", prefix = "/", description = "基础命令")]
pub enum BasicCommands {
    #[plugin(description = "Ping 测试")]
    Ping(Ping),

    #[plugin(description = "查看状态")]
    Status(Status),
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (use DEBUG to see plugin matching details)
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Starting Ayiou Bot with WASM plugin support...");

    // ========================================================================
    // Method 1: Simple static plugins only (original way)
    // ========================================================================
    // let bot = AyiouBot::new()
    //     .plugin::<BasicCommands>()
    //     .run("ws://127.0.0.1:8080")
    //     .await;

    // ========================================================================
    // Method 2: Dynamic plugin registry with WASM support
    // ========================================================================

    // Create dynamic plugin registry
    let registry = Arc::new(DynamicPluginRegistry::new("./plugins"));

    // Register static plugin
    registry.register_static(BasicCommands::default()).await?;
    registry.enable("basic").await?;

    // Load WASM plugin from file
    let wasm_path = Path::new("../wasm-plugin-ts/build/plugin.wasm");
    if wasm_path.exists() {
        info!("Loading WASM plugin from {:?}", wasm_path);

        let runtime = WasmRuntime::new()?;
        let wasm_plugin = runtime.load_plugin(wasm_path).await?;

        info!(
            "Loaded WASM plugin: {} v{}",
            wasm_plugin.meta().name,
            wasm_plugin.meta().version
        );

        // Register and enable the WASM plugin
        registry.register_static(wasm_plugin).await?;
        registry.enable("hello-ts").await?;
    } else {
        info!("WASM plugin not found at {:?}, skipping...", wasm_path);
        info!("To build the WASM plugin, run:");
        info!("  cd ../wasm-plugin-ts && bun install && bun run build");
    }

    // List all registered plugins
    info!("Registered plugins:");
    for (meta, state) in registry.list() {
        info!("  - {} v{} [{:?}] - {}", meta.name, meta.version, state, meta.description);
    }

    // Create dynamic dispatcher
    let dispatcher = DynamicDispatcher::new(registry.clone());

    // Start command handler for runtime plugin control
    let (handler, _cmd_tx) = PluginCommandHandler::new(registry.clone());
    tokio::spawn(handler.run());

    // ========================================================================
    // Connect to OneBot and start event loop
    // ========================================================================

    info!("Connecting to OneBot...");

    // For this example, we'll just demonstrate the setup
    // In production, you would use the full bot:
    //
    AyiouBot::new()
        .run_with_dynamic_dispatcher("ws://192.168.31.180:3001", dispatcher)
        .await;

    // For now, just keep the program running to show it works
    info!("Bot setup complete! Press Ctrl+C to exit.");
    info!("");
    info!("Available commands:");
    info!("  /ping   - Ping test (static plugin)");
    info!("  /status - Check bot status (static plugin)");
    info!("  /hello  - Say hello (WASM plugin)");
    info!("  /hi     - Quick greeting (WASM plugin)");

    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}
