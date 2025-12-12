// Convenience re-exports: use ayiou::prelude::*;
pub use crate::AyiouBot;

pub use crate::core::{
    Args, ArgsParseError, CronSchedule, Dispatcher, Plugin, PluginManager, PluginMetadata,
    RegexValidated,
};

// Dynamic plugin system
pub use crate::core::{
    DynamicDispatcher, DynamicPluginRegistry, PluginCommand, PluginCommandHandler, PluginEntry,
    PluginSource, PluginState,
};

// WASM plugin runtime (requires "wasm" feature)
#[cfg(feature = "wasm")]
pub use crate::core::{WasmPlugin, WasmRuntime};

// Remote plugin loading (requires "remote" feature)
#[cfg(feature = "remote")]
pub use crate::core::{
    InstalledPlugin, PluginConfig, PluginManifest, PluginRepository, RemotePluginLoader,
};

pub use crate::adapter::onebot::v11::ctx::Ctx;

// Re-export derive macros
pub use ayiou_macros::{Args, Plugin};

// Re-export async_trait for CommandHandler impl
pub use async_trait::async_trait;
