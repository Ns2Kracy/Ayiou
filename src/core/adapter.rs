use anyhow::Result;

use crate::core::driver::BoxFuture;

/// Adapter trait for protocol abstraction
///
/// An adapter encapsulates:
/// - Transport layer (driver)
/// - Protocol parsing
/// - Bot instance for API calls
///
/// The adapter owns its driver and manages the full lifecycle.
pub trait Adapter: Send + 'static {
    /// The bot type for API calls
    type Bot: Send + Sync + Clone + 'static;

    /// Adapter name (e.g., "onebot", "satori")
    fn name(&self) -> &'static str;

    /// Get the bot instance for API calls
    fn bot(&self) -> Self::Bot;

    /// Run the adapter (starts driver and event processing)
    fn run(self: Box<Self>) -> BoxFuture<'static, Result<()>>;
}

/// Bot trait for protocol-agnostic bot operations
pub trait BotAdapter: Send + Sync + Clone + 'static {
    /// Get the adapter name this bot belongs to
    fn adapter_name(&self) -> &'static str;
}
