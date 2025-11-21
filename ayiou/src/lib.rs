//! Ayiou Framework
//!
//! An extensible, developer-friendly chat bot framework.

#[forbid(unsafe_code)]
pub mod adapter;
pub mod bot;
pub mod core;
pub mod driver;

/// The prelude module containing common imports.
pub mod prelude {
    pub use crate::bot::AyiouBot;
    pub use crate::core::action::TargetType;
    pub use crate::core::event::EventHandler;
    pub use crate::core::{Adapter, Bot, Context, Driver, Event, Plugin};
    pub use async_trait::async_trait;
    pub use ayiou_macros::{handler, plugin};
    pub use std::sync::Arc;
}
