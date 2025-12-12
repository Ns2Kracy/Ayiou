pub mod driver;
pub mod dynamic;
pub mod plugin;

#[cfg(feature = "remote")]
pub mod remote;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use driver::Driver;
pub use dynamic::{
    DynamicDispatcher, DynamicPluginRegistry, PluginCommand, PluginCommandHandler, PluginEntry,
    PluginSource, PluginState,
};
pub use plugin::{
    Args, ArgsParseError, CronSchedule, Dispatcher, Plugin, PluginManager, PluginMetadata,
    RegexValidated,
};

#[cfg(feature = "remote")]
pub use remote::{
    InstalledPlugin, PluginConfig, PluginManifest, PluginRepository, RemotePluginLoader,
};

#[cfg(feature = "wasm")]
pub use wasm::{WasmPlugin, WasmRuntime};
