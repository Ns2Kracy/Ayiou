use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use config::{Config as ConfigBuilder, Environment, File as ConfigFile};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tracing::info;

use crate::{app::AppConfig, error::ConfigError};

// Global configuration manager instance
static CONFIG_MANAGER: OnceCell<ConfigManager> = OnceCell::new();

// Default configuration path
const DEFAULT_CONFIG_PATH: &str = "config/ayiah.toml";
const ENVIRONMENT_PREFIX: &str = "AYIAH";

/// Configuration manager
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new configuration manager instance
    pub fn new<P: AsRef<Path>>(config_path: Option<P>) -> Result<Self, ConfigError> {
        let config_path = config_path
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        let config = Self::load_config(&config_path)?;
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        })
    }

    /// Initialize the global configuration manager instance
    pub fn init<P: AsRef<Path>>(config_path: Option<P>) -> Result<&'static Self, ConfigError> {
        let config_path = config_path
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        info!("Initializing configuration from {:?}", config_path);

        let manager = CONFIG_MANAGER.get_or_init(|| match Self::new(Some(&config_path)) {
            Ok(manager) => manager,
            Err(e) => {
                panic!("Failed to initialize configuration: {}", e);
            }
        });

        Ok(manager)
    }

    /// Get the global configuration manager instance
    pub fn instance() -> Result<&'static Self, ConfigError> {
        CONFIG_MANAGER.get().ok_or(ConfigError::NotInitialized)
    }

    /// Get a read lock on the configuration
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, AppConfig> {
        self.config.read()
    }

    /// Get a write lock on the configuration
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, AppConfig> {
        self.config.write()
    }

    /// Reload the configuration
    pub fn reload(&self) -> Result<(), ConfigError> {
        let new_config = Self::load_config(&self.config_path)?;
        let mut config = self.config.write();
        *config = new_config;
        info!("Configuration reloaded successfully");
        Ok(())
    }

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
                            "Failed to create configuration directory: {}",
                            e
                        ))
                    })?;
                }
            }

            let default_config = AppConfig::default();
            let toml_str = toml::to_string_pretty(&default_config)
                .map_err(|e| ConfigError::ParseError(e.to_string()))?;

            fs::write(config_path, toml_str).map_err(|e| {
                ConfigError::WriteError(format!("Failed to write configuration file: {}", e))
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
}
