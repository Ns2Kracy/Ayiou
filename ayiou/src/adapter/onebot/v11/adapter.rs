use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    adapter::onebot::v11::model::{Message, MessageEvent, OneBotEvent},
    core::Driver,
    driver::wsclient::WsDriver,
};

/// OneBot v11 Adapter utilities
///
/// Provides a single entry point to start the transport + protocol stack
pub struct OneBotV11Adapter;

impl OneBotV11Adapter {
    /// Start the adapter and return the outgoing channel for API calls
    pub fn start(
        url: impl Into<String>,
        event_tx: mpsc::Sender<OneBotEvent>,
    ) -> mpsc::Sender<String> {
        let url = url.into();
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        let (raw_tx, mut raw_rx) = mpsc::channel::<String>(100);

        tokio::spawn(async move {
            let driver = WsDriver::new(&url, raw_tx, outgoing_rx);
            let driver_handle = tokio::spawn(async move {
                if let Err(e) = Box::new(driver).run().await {
                    error!("Driver error: {}", e);
                }
            });

            while let Some(raw) = raw_rx.recv().await {
                // OneBot WS contains both:
                // - Events: {"post_type": "message" | "notice" | ...}
                // - Action responses: {"status": "ok", "retcode": 0, ...}
                // We should only forward events into event_tx.
                if raw.contains("\"post_type\"") {
                    match serde_json::from_str::<OneBotEvent>(&raw) {
                        Ok(event) => {
                            if let OneBotEvent::Message(msg_event) = &event {
                                Self::log_message(msg_event);
                            }

                            if event_tx.send(event).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse event: {}, raw: {}", e, raw);
                        }
                    }
                } else {
                    // Ignore action responses for now.
                    // (Future: route by echo to awaiting callers)
                    tracing::debug!("Ignoring OneBot action response: {}", raw);
                }
            }

            driver_handle.abort();
        });

        outgoing_tx
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
                    "群聊 [{}] [{}({})] {}",
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
