// Convenience re-exports: use ayiou::prelude::*;
pub use crate::AyiouBot;

// Core types
pub use crate::core::{
    // App system
    App,
    AppBuilder,
    AppState,
    PluginGroup,
    ResourceRegistry,
    // Configuration
    BotConfig,
    ConfigStore,
    Configurable,
    // Lifecycle
    LifecycleDriver,
    get_driver,
    // Plugin system
    Args,
    ArgsParseError,
    CronSchedule,
    Dispatcher,
    Plugin,
    PluginBox,
    PluginDependency,
    PluginList,
    PluginManager,
    PluginMetadata,
    RegexValidated,
};

pub use crate::adapter::onebot::v11::ctx::Ctx;

// Re-export derive macros
pub use ayiou_macros::{Args, Plugin};

// Re-export async_trait for Plugin impl
pub use async_trait::async_trait;

// Re-export anyhow for error handling
pub use anyhow::{anyhow, Result};
