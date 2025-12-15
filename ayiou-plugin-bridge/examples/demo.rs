//! Bridge Plugin Demo
//!
//! This example demonstrates loading the ExternalPluginBridge and 
//! configuring it to run a Python script.

use ayiou::prelude::*;
use ayiou_plugin_bridge::ExternalPluginBridge;
use std::path::PathBuf;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Bridge Plugin Demo...");

    // Find the python script
    // We assume running from workspace root
    let mut script_path = PathBuf::from("examples");
    script_path.push("external_plugin.py");
    
    // If not found, try finding relatively
    if !script_path.exists() {
         script_path = PathBuf::from("..").join("examples").join("external_plugin.py");
    }

    let script_path_str = std::fs::canonicalize(&script_path)
        .unwrap_or(script_path.clone())
        .to_string_lossy()
        .to_string();
        
    info!("Using python script: {}", script_path_str);

    // Create AppBuilder
    let mut builder = AppBuilder::new();
    
    // Create config file
    let config_file_path = "bridge_config.toml";
    let config_toml = format!(
        r#"
[external-plugin-bridge]
enabled = true

[external-plugin-bridge.plugins.python-demo]
command = "python"
args = ["-u", "{}"]
"#,
        script_path_str.replace("\\", "\\\\")
    );
    
    std::fs::write(config_file_path, config_toml)?;
    info!("Written config to {}", config_file_path);
    
    // Load config
    builder = builder.config_file(config_file_path)?;
    
    // Add the bridge plugin
    let bridge = ExternalPluginBridge::new();
    builder.add_plugin(bridge)?;
    
    info!("Building app...");
    let mut app = builder.build().await?;
    
    // Keep running to allow python plugin to interaction
    info!("App running. Waiting 5s...");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    
    // Cleanup
    app.shutdown().await?;
    std::fs::remove_file(config_file_path).ok();
    
    Ok(())
}
