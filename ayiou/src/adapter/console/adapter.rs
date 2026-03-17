use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    adapter::console::{ctx::Ctx, sender::ConsoleSender},
    core::{
        adapter::{Adapter, AdapterRuntime, ProtocolAdapter},
        context::Context,
        driver::Driver,
    },
    driver::console::ConsoleDriver,
};

pub struct ConsoleAdapter {
    driver: Box<dyn Driver<Inbound = String, Outbound = String>>,
}

impl ConsoleAdapter {
    pub fn new() -> Self {
        Self {
            driver: Box::new(ConsoleDriver::new()),
        }
    }

    pub fn with_driver<D>(driver: D) -> Self
    where
        D: Driver<Inbound = String, Outbound = String>,
    {
        Self {
            driver: Box::new(driver),
        }
    }
}

impl Default for ConsoleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

struct ConsoleProtocol;

#[async_trait]
impl ProtocolAdapter for ConsoleProtocol {
    type Inbound = String;
    type Outbound = String;
    type Ctx = Context;

    async fn handle_packet(
        &mut self,
        raw: Self::Inbound,
        outbound_tx: mpsc::Sender<Self::Outbound>,
    ) -> Option<Self::Ctx> {
        let text = raw.trim();
        if text.is_empty() {
            return None;
        }

        let sender = std::sync::Arc::new(ConsoleSender::new(outbound_tx.clone()));
        Some(Ctx::new(raw).into_context(Some(sender)))
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    type Ctx = Context;

    fn capabilities(&self) -> crate::core::adapter::AdapterCapabilities {
        crate::core::adapter::AdapterCapabilities {
            proactive_send: true,
            attachments: false,
            platform_extensions: vec!["console".to_string()],
        }
    }

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        self.start_with_runtime().await.events
    }

    async fn start_with_runtime(self) -> AdapterRuntime<Self::Ctx> {
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        let (raw_tx, mut raw_rx) = mpsc::channel::<String>(100);
        let (ctx_tx, ctx_rx) = mpsc::channel::<Context>(100);
        let driver = self.driver;
        let protocol_outgoing_tx = outgoing_tx.clone();
        let mut protocol = ConsoleProtocol;

        tokio::spawn(async move {
            let driver_handle = tokio::spawn(async move {
                if let Err(err) = driver.run(raw_tx, outgoing_rx).await {
                    log::error!("Driver error: {}", err);
                }
            });

            while let Some(raw) = raw_rx.recv().await {
                if let Some(ctx) = protocol
                    .handle_packet(raw, protocol_outgoing_tx.clone())
                    .await
                    && ctx_tx.send(ctx).await.is_err()
                {
                    break;
                }
            }

            driver_handle.abort();
        });

        AdapterRuntime {
            events: ctx_rx,
            sender: Some(std::sync::Arc::new(ConsoleSender::new(outgoing_tx))),
        }
    }
}
