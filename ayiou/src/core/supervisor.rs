mod api;
mod catalog;
mod types;

pub use api::{BotManager, ConfigManager, PluginManagerApi, Supervisor};
pub use catalog::{ManagedPlugin, PluginCatalog, PluginFactory};
pub use types::{
    BotDefinition, BotStatus, PluginConfigSnapshot, PluginHealth, PluginInstanceSpec,
    RuntimeServices,
};
