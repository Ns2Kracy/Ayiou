// Convenience re-exports: use ayiou::prelude::*;
pub use crate::AyiouBot;

pub use crate::core::{Dispatcher, Plugin, PluginManager, PluginMetadata};

pub use crate::adapter::onebot::v11::ctx::Ctx;

// Re-export the Plugin derive macro
pub use ayiou_macros::Plugin;

// Re-export async_trait for CommandHandler impl
pub use async_trait::async_trait;
