use anyhow::Result;
use async_trait::async_trait;

/// Represents a Bot instance that can perform actions.
/// Adapters implement this trait.
#[async_trait]
pub trait Bot: Send + Sync {
    /// Get the self ID of the bot.
    fn self_id(&self) -> &str;

    /// Send a text message to a user or group.
    /// For complex messages (images, embeds), we would define a `MessageChain` struct.
    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String>; // Returns Message ID
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Private,
    Group,
    Channel,
}
