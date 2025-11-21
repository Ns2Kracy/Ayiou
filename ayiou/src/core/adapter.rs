use crate::core::action::TargetType;
use anyhow::Result;
use async_trait::async_trait;

/// An Adapter bridges the Driver and the Core App.
/// It translates raw events from a driver into core Ayiou events,
/// and serializes core actions into raw data for a driver to send.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Returns the name of the adapter (e.g., "onebot").
    fn name(&self) -> &'static str;

    /// Handles a raw event string from a driver and translates it into a core event.
    /// The implementation should push the resulting core event into the event bus (via context).
    async fn handle(&self, raw_event: String) -> Result<()>;

    /// Serializes a generic `send_message` action into a platform-specific raw string.
    fn serialize(&self, target_id: &str, target_type: TargetType, content: &str) -> Result<String>;
}
