use std::future::Future;

use async_trait::async_trait;
use log::error;
use tokio::sync::mpsc;

use crate::core::driver::Driver;

/// Trait for contexts that support message operations.
pub trait MsgContext: Send + Sync + Clone + 'static {
    fn text(&self) -> String;
    fn user_id(&self) -> String;
    fn group_id(&self) -> Option<String>;
}

/// High-level adapter trait.
///
/// Adapter is responsible for protocol translation:
/// - raw inbound packet -> context/event
/// - context action -> raw outbound packet
#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    type Ctx: MsgContext;

    /// Start adapter and return a stream of normalized contexts.
    async fn start(self) -> mpsc::Receiver<Self::Ctx>;
}

/// Protocol adapter abstraction:
///
/// - receives raw packets from a driver
/// - converts to normalized contexts
/// - can use outbound sender to emit raw actions
#[async_trait]
pub trait ProtocolAdapter: Send + 'static {
    type Inbound: Send + 'static;
    type Outbound: Send + 'static;
    type Ctx: Send + 'static;

    async fn handle_packet(
        &mut self,
        raw: Self::Inbound,
        outbound_tx: mpsc::Sender<Self::Outbound>,
    ) -> Option<Self::Ctx>;
}

/// Spawn a generic driver+adapter loop.
///
/// This utility wires a driver and an async packet handler together:
/// - driver handles raw connection lifecycle
/// - handler performs protocol translation
pub fn spawn_driver_adapter<I, O, Ctx, H, Fut>(
    driver: Box<dyn Driver<Inbound = I, Outbound = O>>,
    buffer: usize,
    mut handler: H,
) -> mpsc::Receiver<Ctx>
where
    I: Send + 'static,
    O: Send + 'static,
    Ctx: Send + 'static,
    H: FnMut(I, mpsc::Sender<O>) -> Fut + Send + 'static,
    Fut: Future<Output = Option<Ctx>> + Send,
{
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<O>(buffer);
    let (raw_tx, mut raw_rx) = mpsc::channel::<I>(buffer);
    let (ctx_tx, ctx_rx) = mpsc::channel::<Ctx>(buffer);

    tokio::spawn(async move {
        let driver_handle = tokio::spawn(async move {
            if let Err(err) = driver.run(raw_tx, outgoing_rx).await {
                error!("Driver error: {}", err);
            }
        });

        while let Some(raw) = raw_rx.recv().await {
            if let Some(ctx) = handler(raw, outgoing_tx.clone()).await
                && ctx_tx.send(ctx).await.is_err()
            {
                break;
            }
        }

        driver_handle.abort();
    });

    ctx_rx
}

/// Spawn a driver + protocol adapter pair.
pub fn spawn_protocol_adapter<P>(
    driver: Box<dyn Driver<Inbound = P::Inbound, Outbound = P::Outbound>>,
    buffer: usize,
    mut protocol: P,
) -> mpsc::Receiver<P::Ctx>
where
    P: ProtocolAdapter,
{
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<P::Outbound>(buffer);
    let (raw_tx, mut raw_rx) = mpsc::channel::<P::Inbound>(buffer);
    let (ctx_tx, ctx_rx) = mpsc::channel::<P::Ctx>(buffer);

    tokio::spawn(async move {
        let driver_handle = tokio::spawn(async move {
            if let Err(err) = driver.run(raw_tx, outgoing_rx).await {
                error!("Driver error: {}", err);
            }
        });

        while let Some(raw) = raw_rx.recv().await {
            if let Some(ctx) = protocol.handle_packet(raw, outgoing_tx.clone()).await
                && ctx_tx.send(ctx).await.is_err()
            {
                break;
            }
        }

        driver_handle.abort();
    });

    ctx_rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::mock::MockDriver;

    struct EchoProtocol;

    #[async_trait]
    impl ProtocolAdapter for EchoProtocol {
        type Inbound = String;
        type Outbound = String;
        type Ctx = String;

        async fn handle_packet(
            &mut self,
            raw: Self::Inbound,
            _outgoing_tx: mpsc::Sender<Self::Outbound>,
        ) -> Option<Self::Ctx> {
            Some(format!("ctx:{}", raw))
        }
    }

    #[tokio::test]
    async fn protocol_adapter_bridges_driver_packets() {
        let driver = Box::new(MockDriver::<String, String>::new(vec![
            "a".to_string(),
            "b".to_string(),
        ]));

        let mut rx = spawn_protocol_adapter(driver, 8, EchoProtocol);

        assert_eq!(rx.recv().await.unwrap(), "ctx:a");
        assert_eq!(rx.recv().await.unwrap(), "ctx:b");
        assert!(rx.recv().await.is_none());
    }
}
