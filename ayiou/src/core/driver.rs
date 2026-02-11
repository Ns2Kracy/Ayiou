use tokio::sync::mpsc;

/// Driver trait for transport layer abstraction.
///
/// Implementors are responsible for platform connection lifecycle only:
/// connect/reconnect, heartbeat, and raw packet I/O.
#[async_trait::async_trait]
pub trait Driver: Send + Sync + 'static {
    type Inbound: Send + 'static;
    type Outbound: Send + 'static;

    async fn run(
        self: Box<Self>,
        inbound_tx: mpsc::Sender<Self::Inbound>,
        outbound_rx: mpsc::Receiver<Self::Outbound>,
    ) -> anyhow::Result<()>;
}
