use std::{
    env, fs,
    path::{Path, PathBuf},
};

use config::{Config as ConfigBuilder, Environment, File as ConfigFile};
use once_cell::sync::Lazy;
use tracing::info;

use crate::{app::AppConfig, error::ConfigError};

// Default configuration path
const DEFAULT_CONFIG_PATH: &str = "ayiou.toml";
const ENVIRONMENT_PREFIX: &str = "AYIOU";

// Global configuration instance with automatic initialization
pub static CONFIG: Lazy<AppConfig> = Lazy::new(|| {
    let config_path = env::var("AYIOU_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH));

    info!("Loading configuration from {:?}", config_path);

    load_config(&config_path).unwrap_or_else(|e| {
        panic!("Failed to load configuration: {e}");
    })
});

/// Load configuration from file and environment variables
fn load_config<P: AsRef<Path>>(config_path: P) -> Result<AppConfig, ConfigError> {
    let config_path = config_path.as_ref();

    // Check if the configuration file exists, if not, create default configuration
    if !config_path.exists() {
        info!(
            "Configuration file not found, creating default configuration at {:?}",
            config_path
        );
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    ConfigError::WriteError(format!(
                        "Failed to create configuration directory: {e}"
                    ))
                })?;
            }
        }

        let default_config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&default_config)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        fs::write(config_path, toml_str).map_err(|e| {
            ConfigError::WriteError(format!("Failed to write configuration file: {e}"))
        })?;
    }

    // Build configuration, combining file and environment variables
    let config = ConfigBuilder::builder()
        // Load from default file
        .add_source(ConfigFile::from(config_path))
        // Load from environment variables with higher priority
        .add_source(
            Environment::with_prefix(ENVIRONMENT_PREFIX)
                .separator("__")
                .try_parsing(true),
        )
        .build()?;

    // Deserialize the configuration
    let app_config: AppConfig = config.try_deserialize()?;
    Ok(app_config)
}
