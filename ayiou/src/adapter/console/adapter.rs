use async_trait::async_trait;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::{
    adapter::console::{ctx::Ctx, sender::ConsoleSender},
    core::{
        adapter::{Adapter, AdapterRuntime},
        context::Context,
        driver::Driver,
        plugin::{Capability, OutboundSender},
    },
    driver::console::ConsoleDriver,
};

pub struct ConsoleAdapter {
    driver: Box<dyn Driver<Inbound = String, Outbound = String>>,
}

impl ConsoleAdapter {
    #[must_use]
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

struct ConsoleProtocol {
    sender: Arc<dyn OutboundSender>,
}

impl ConsoleProtocol {
    fn handle_packet(&mut self, raw: String) -> Option<Context> {
        let text = raw.trim();
        if text.is_empty() {
            return None;
        }

        Some(Ctx::new(raw).into_context(Some(self.sender.clone())))
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    async fn start(self) -> AdapterRuntime {
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        let (raw_tx, mut raw_rx) = mpsc::channel::<String>(100);
        let (ctx_tx, ctx_rx) = mpsc::channel::<Context>(100);
        let driver = self.driver;
        let sender = Arc::new(ConsoleSender::new(outgoing_tx)) as Arc<dyn OutboundSender>;
        let mut protocol = ConsoleProtocol {
            sender: sender.clone(),
        };

        tokio::spawn(async move {
            let driver_handle = tokio::spawn(async move {
                if let Err(err) = driver.run(raw_tx, outgoing_rx).await {
                    log::error!("Driver error: {err}");
                }
            });

            while let Some(raw) = raw_rx.recv().await {
                if let Some(ctx) = protocol.handle_packet(raw)
                    && ctx_tx.send(ctx).await.is_err()
                {
                    break;
                }
            }

            driver_handle.abort();
        });

        AdapterRuntime {
            events: ctx_rx,
            sender: Some(sender),
            capabilities: vec![Capability::ProactiveSend, Capability::custom("console")],
        }
    }
}
