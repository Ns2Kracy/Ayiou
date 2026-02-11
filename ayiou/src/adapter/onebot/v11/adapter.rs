use std::sync::{Arc, atomic::AtomicU64};

use async_trait::async_trait;
use dashmap::DashMap;
use log::{info, warn};
use tokio::sync::{mpsc, oneshot};

use crate::{
    adapter::onebot::v11::ctx::Ctx,
    adapter::onebot::v11::model::{ApiResponse, Message, MessageEvent, OneBotEvent},
    core::{
        adapter::{Adapter, ProtocolAdapter, spawn_protocol_adapter},
        driver::Driver,
    },
    driver::wsclient::WsDriver,
};

/// OneBot v11 protocol adapter.
///
/// Adapter responsibilities:
/// - convert raw packets to OneBot events
/// - map OneBot responses by echo
/// - build plugin context from message events
pub struct OneBotV11Adapter {
    driver: Box<dyn Driver<Inbound = String, Outbound = String>>,
}

impl OneBotV11Adapter {
    pub fn new(url: impl Into<String>) -> Self {
        Self::with_driver(WsDriver::new(&url.into()))
    }

    pub fn with_driver<D>(driver: D) -> Self
    where
        D: Driver<Inbound = String, Outbound = String>,
    {
        Self {
            driver: Box::new(driver),
        }
    }

    fn log_message(msg_event: &MessageEvent) {
        match msg_event {
            MessageEvent::Private(p) => {
                info!(
                    "私聊 [{}({})] {}",
                    p.sender.nickname,
                    p.user_id,
                    Self::format_message(&p.message)
                )
            }
            MessageEvent::Group(g) => {
                info!(
                    "群聊 [{}({})] [{}({})] {}",
                    g.group_name,
                    g.group_id,
                    g.sender.card.as_deref().unwrap_or(&g.sender.nickname),
                    g.user_id,
                    Self::format_message(&g.message)
                )
            }
        };
    }

    fn format_message(message: &Message) -> String {
        match message {
            Message::String(s) => format!("{:?}", s),
            Message::Array(segments) => {
                let mut preview = String::with_capacity(segments.len() * 8);
                for seg in segments {
                    seg.write_preview(&mut preview);
                }
                format!("{:?}", preview)
            }
        }
    }
}

struct OneBotV11Protocol {
    pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
    echo_seq: Arc<AtomicU64>,
}

impl OneBotV11Protocol {
    fn new() -> Self {
        Self {
            pending_api: Arc::new(DashMap::new()),
            echo_seq: Arc::new(AtomicU64::new(1)),
        }
    }
}

#[async_trait]
impl ProtocolAdapter for OneBotV11Protocol {
    type Inbound = String;
    type Outbound = String;
    type Ctx = Ctx;

    async fn handle_packet(
        &mut self,
        raw: Self::Inbound,
        outgoing_tx: mpsc::Sender<Self::Outbound>,
    ) -> Option<Self::Ctx> {
        if let Ok(resp) = serde_json::from_str::<ApiResponse>(&raw) {
            if let Some(echo) = &resp.echo {
                if let Some((_, tx)) = self.pending_api.remove(echo) {
                    let _ = tx.send(resp);
                } else {
                    warn!("Received OneBot response with unknown echo: {}", echo);
                }
            }
            return None;
        }

        match serde_json::from_str::<OneBotEvent>(&raw) {
            Ok(event) => {
                if let OneBotEvent::Message(msg_event) = &event {
                    OneBotV11Adapter::log_message(msg_event);
                }

                let event = Arc::new(event);
                Ctx::new(
                    event,
                    outgoing_tx,
                    self.pending_api.clone(),
                    self.echo_seq.clone(),
                )
            }
            Err(err) => {
                warn!("Failed to parse: {}, raw: {}", err, raw);
                None
            }
        }
    }
}

#[async_trait]
impl Adapter for OneBotV11Adapter {
    type Ctx = Ctx;

    async fn start(self) -> mpsc::Receiver<Ctx> {
        spawn_protocol_adapter(self.driver, 100, OneBotV11Protocol::new())
    }
}
