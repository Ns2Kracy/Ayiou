/// Driver trait for transport layer abstraction
///
/// Implementors handle the actual network transport (WebSocket, HTTP, etc.)
/// without any protocol-specific logic.
#[async_trait::async_trait]
pub trait Driver: Send + 'static {
    async fn run(self: Box<Self>) -> anyhow::Result<()>;
}
