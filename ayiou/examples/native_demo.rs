//! Native Plugin Demo
//!
//! This example demonstrates the new plugin features:
//! - Lifecycle hooks (build, ready, finish, cleanup)
//! - Dependency declarations via macro arguments
//! - AppBuilder integration

use ayiou::prelude::*;
use tokio::time::Duration;
use tracing::{info, Level};

// Define a shared resource
struct DatabaseConnection {
    connected: bool,
}

// ============================================================================
// Plugin 1: Database (Dependency)
// ============================================================================

#[derive(Default)]
struct DatabasePlugin;

#[async_trait]
impl Plugin for DatabasePlugin {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("database").description("Mock Database Plugin")
    }

    async fn build(&self, app: &mut AppBuilder) -> Result<()> {
        info!("[Database] Building...");
        // Simulate heavy initialization
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Insert a shared resource
        app.insert_resource(DatabaseConnection { connected: true });
        info!("[Database] Resource inserted!");
        Ok(())
    }

    async fn finish(&self, _app: &mut App) -> Result<()> {
        info!("[Database] Finished initialization!");
        Ok(())
    }

    async fn cleanup(&self, _app: &mut App) -> Result<()> {
        info!("[Database] Cleaning up connections...");
        Ok(())
    }

    async fn handle(&self, _ctx: &Ctx) -> Result<bool> {
        Ok(false)
    }
}

// ============================================================================
// Plugin 2: User System (Depends on Database)
// ============================================================================

#[derive(Default)]
struct UserPlugin;

#[async_trait]
impl Plugin for UserPlugin {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("users")
            .description("User Management")
            .version("0.1.0")
    }

    fn dependencies(&self) -> Vec<PluginDependency> {
        vec![PluginDependency::required("database")]
    }

    async fn build(&self, app: &mut AppBuilder) -> Result<()> {
        info!("[Users] Building... Waiting for database.");
        
        // Check if dependency resource exists
        if app.get_resource::<DatabaseConnection>().is_none() {
            return Err(anyhow::anyhow!("Database connection missing! Dependency resolution failed?"));
        }
        
        info!("[Users] Database connection found!");
        Ok(())
    }

    async fn handle(&self, ctx: &Ctx) -> Result<bool> {
        if ctx.text() == "ping" {
            ctx.reply_text("pong from native plugin!").await?;
            return Ok(true);
        }
        Ok(false)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Native Plugin Demo...");

    // Setup global driver hooks
    {
        let mut driver = get_driver().write().await;
        driver.on_startup(|| async {
            info!("🚀 Global Startup Hook Triggered!");
        });
        driver.on_shutdown(|| async {
            info!("🛑 Global Shutdown Hook Triggered!");
        });
    }

    // Run the bot
    // Note: In a real app, you'd use .run(url). Here we just build to demo lifecycle.
    
    let mut builder = AppBuilder::new();
    
    // Add plugins (order shouldn't matter due to topological sort)
    builder.add_plugin(UserPlugin)?;     // Depends on database
    builder.add_plugin(DatabasePlugin)?; // Provide database

    info!("Building app (Lifecycle: build -> ready -> finish)...");
    let mut app = builder.build().await?;
    
    info!("App is ready! Press Ctrl+C to test shutdown hooks.");
    
    // Simulate running for a bit
    // In a real scenario, AyiouBot::run() does this loop for you
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received");
        }
        _ = tokio::time::sleep(Duration::from_secs(2)) => {
            info!("Demo timeout reached");
        }
    }

    info!("Shutting down app (Lifecycle: cleanup)...");
    app.shutdown().await?;
    
    // Run global shutdown hooks manually if not using AyiouBot::run
    get_driver().read().await.run_shutdown_hooks().await;

    Ok(())
}
