use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Driver event contains raw data from the underlying protocol.
#[derive(Debug)]
pub enum DriverEvent {
    Connect,
    Disconnect,
    // For console, it's a string line. For HTTP/WS, it might be bytes or Text.
    Message(String),
}

/// A Driver is responsible for low-level I/O (Network, Stdin, etc).
/// It pushes raw events to a channel.
#[async_trait]
pub trait Driver: Send + Sync {
    /// Start the driver. It should loop and push events to the sender.
    async fn start(&self, tx: mpsc::Sender<DriverEvent>) -> Result<()>;

    /// Send raw data back to the client/user.
    async fn send(&self, content: String) -> Result<()>;
}
