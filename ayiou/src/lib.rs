#![allow(clippy::missing_errors_doc, clippy::multiple_crate_versions)]

#[cfg(any(feature = "adapter-console", feature = "adapter-onebot-v11"))]
pub mod adapter;
pub mod bot;
#[cfg(feature = "control-plane")]
pub mod control_plane;
pub mod core;
#[cfg(any(
    feature = "driver-console",
    feature = "driver-wsclient",
    feature = "driver-mock"
))]
pub mod driver;

pub use ayiou_macros::{command, plugin};
#[cfg(feature = "adapter-console")]
pub use bot::ConsoleBot;
#[cfg(feature = "adapter-onebot-v11")]
pub use bot::OneBotV11Bot;
pub use bot::{Bot, BotRuntimeOptions, QueueOverflowPolicy};
#[cfg(feature = "control-plane")]
pub use control_plane::ControlPlaneOptions;
pub use core::context::Context;
pub use core::model::*;
pub use core::runtime::{RuntimeController, RuntimeState};
pub use inventory;
