use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    adapter::console::ctx::Ctx,
    core::{
        adapter::{Adapter, ProtocolAdapter, spawn_protocol_adapter},
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
    type Ctx = Ctx;

    async fn handle_packet(
        &mut self,
        raw: Self::Inbound,
        outbound_tx: mpsc::Sender<Self::Outbound>,
    ) -> Option<Self::Ctx> {
        let text = raw.trim();
        if text.is_empty() {
            return None;
        }

        Some(Ctx::new(raw, outbound_tx))
    }
}

#[async_trait]
impl Adapter for ConsoleAdapter {
    type Ctx = Ctx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        spawn_protocol_adapter(self.driver, 100, ConsoleProtocol)
    }
}
