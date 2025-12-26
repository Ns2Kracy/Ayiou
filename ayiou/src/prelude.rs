// Convenience re-exports: use ayiou::prelude::*;
pub use crate::AyiouBot;

pub use crate::core::driver::Driver;
pub use crate::core::extract::{Args, Rest, TupleArgs};
pub use crate::core::plugin::{
    ArgsParseError, ArgsParser, Command, CronSchedule, Dispatcher, Plugin, PluginManager,
    PluginMetadata, RegexValidated,
};

pub use crate::adapter::onebot::v11::ctx::Ctx;

// Re-export derive macros
pub use ayiou_macros::Plugin;

// Re-export async_trait for CommandHandler impl
pub use async_trait::async_trait;
