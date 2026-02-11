use std::marker::PhantomData;

use tokio::sync::mpsc;

use crate::core::driver::Driver;

/// In-memory driver for tests.
///
/// It pushes predefined inbound packets and ignores outbound packets.
pub struct MockDriver<I, O> {
    inbound_packets: Vec<I>,
    _outbound: PhantomData<O>,
}

impl<I, O> MockDriver<I, O> {
    pub fn new(inbound_packets: Vec<I>) -> Self {
        Self {
            inbound_packets,
            _outbound: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<I, O> Driver for MockDriver<I, O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    type Inbound = I;
    type Outbound = O;

    async fn run(
        self: Box<Self>,
        inbound_tx: mpsc::Sender<Self::Inbound>,
        _outbound_rx: mpsc::Receiver<Self::Outbound>,
    ) -> anyhow::Result<()> {
        for packet in self.inbound_packets {
            if inbound_tx.send(packet).await.is_err() {
                break;
            }
        }
        Ok(())
    }
}
