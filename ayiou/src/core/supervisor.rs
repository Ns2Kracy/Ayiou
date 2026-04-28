mod api;
mod catalog;
mod types;

pub use api::{BotManager, ConfigManager, PluginManagerApi, Supervisor};
pub use catalog::PluginCatalog;
pub use types::{
    BotDefinition, BotStatus, PluginConfigSnapshot, PluginHealth, PluginInstanceSpec,
    RuntimeServices,
};
