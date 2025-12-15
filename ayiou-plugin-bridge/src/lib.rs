//! External Plugin Bridge for Ayiou
//!
//! This plugin allows running external plugins written in any language
//! (Python, JavaScript, Go, etc.) through a JSON-RPC protocol over stdio.
//!
//! # Configuration
//!
//! ```toml
//! [external-plugin-bridge]
//! enabled = true
//!
//! [external-plugin-bridge.plugins.weather]
//! command = "uv"
//! args = ["run", "ayiou-plugin-weather"]
//!
//! [external-plugin-bridge.plugins.dalle]
//! command = "bun"
//! args = ["run", "./plugins/dalle/index.ts"]
//! ```

mod config;
mod process;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ayiou::core::{app::AppBuilder, Plugin, PluginMetadata};
use ayiou::adapter::onebot::v11::ctx::Ctx;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

pub use config::{BridgeConfig, PluginConfig};
use process::ExternalProcess;

/// External Plugin Bridge
///
/// Acts as a bridge between Ayiou core and external plugins
/// running as separate processes.
pub struct ExternalPluginBridge {
    processes: Arc<RwLock<HashMap<String, ExternalProcess>>>,
}

impl Default for ExternalPluginBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalPluginBridge {
    /// Create a new bridge
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Plugin for ExternalPluginBridge {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("external-plugin-bridge")
            .description("Multi-language external plugin support")
            .version("0.1.0")
    }

    fn is_unique(&self) -> bool {
        true
    }

    async fn build(&self, app: &mut AppBuilder) -> Result<()> {
        let config: BridgeConfig = app.config()?;

        if !config.enabled {
            info!("External plugin bridge is disabled");
            return Ok(());
        }

        info!("Starting {} external plugins", config.plugins.len());

        let mut processes = self.processes.write().await;

        for (name, plugin_config) in config.plugins {
            info!("Starting external plugin: {}", name);
            
            match ExternalProcess::spawn(&plugin_config).await {
                Ok(process) => {
                    // Send startup lifecycle event
                    if let Err(e) = process.send_lifecycle_startup().await {
                        warn!("Failed to send startup to {}: {}", name, e);
                    }
                    processes.insert(name.clone(), process);
                    info!("External plugin {} started successfully", name);
                }
                Err(e) => {
                    error!("Failed to start external plugin {}: {}", name, e);
                }
            }
        }

        Ok(())
    }

    fn matches(&self, _ctx: &Ctx) -> bool {
        // Always try to match if we have processes
        true
    }

    async fn handle(&self, ctx: &Ctx) -> Result<bool> {
        let processes = self.processes.read().await;

        if processes.is_empty() {
            return Ok(false);
        }

        for (name, process) in processes.iter() {
            // Check if this plugin matches
            match process.call_matches(ctx).await {
                Ok(true) => {
                    debug!("External plugin {} matches", name);
                    
                    // Call handle
                    match process.call_handle(ctx).await {
                        Ok(result) => {
                            // Send reply if present
                            if let Some(reply) = result.reply {
                                ctx.reply_text(reply).await?;
                            }
                            
                            if result.block {
                                return Ok(true);
                            }
                        }
                        Err(e) => {
                            warn!("External plugin {} handle error: {}", name, e);
                        }
                    }
                }
                Ok(false) => {
                    debug!("External plugin {} does not match", name);
                }
                Err(e) => {
                    warn!("External plugin {} matches error: {}", name, e);
                }
            }
        }

        Ok(false)
    }

    async fn cleanup(&self, _app: &mut ayiou::core::App) -> Result<()> {
        info!("Shutting down external plugins");
        
        let mut processes = self.processes.write().await;
        
        for (name, process) in processes.drain() {
            info!("Stopping external plugin: {}", name);
            
            // Send shutdown event
            if let Err(e) = process.send_lifecycle_shutdown().await {
                warn!("Failed to send shutdown to {}: {}", name, e);
            }
            
            // Kill the process
            if let Err(e) = process.kill().await {
                warn!("Failed to kill {}: {}", name, e);
            }
        }
        
        Ok(())
    }
}
