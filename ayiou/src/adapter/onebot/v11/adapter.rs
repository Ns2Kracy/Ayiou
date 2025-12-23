use std::sync::Arc;

use async_trait::async_trait;
use log::{error, info, warn};
use tokio::sync::mpsc;

use crate::{
    adapter::onebot::v11::ctx::Ctx,
    adapter::onebot::v11::model::{Message, MessageEvent, OneBotEvent},
    core::{adapter::Adapter, driver::Driver},
    driver::wsclient::WsDriver,
};

/// OneBot v11 Adapter utilities
///
/// Provides a single entry point to start the transport + protocol stack
pub struct OneBotV11Adapter {
    pub url: String,
}

impl OneBotV11Adapter {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
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

#[async_trait]
impl Adapter for OneBotV11Adapter {
    type Ctx = Ctx;

    async fn start(self) -> mpsc::Receiver<Ctx> {
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        let (raw_tx, mut raw_rx) = mpsc::channel::<String>(100);
        let (ctx_tx, ctx_rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let driver = WsDriver::new(&self.url, raw_tx, outgoing_rx);
            let driver_handle = tokio::spawn(async move {
                if let Err(e) = Box::new(driver).run().await {
                    error!("Driver error: {}", e);
                }
            });

            while let Some(raw) = raw_rx.recv().await {
                match serde_json::from_str::<OneBotEvent>(&raw) {
                    Ok(event) => {
                        if let OneBotEvent::Message(msg_event) = &event {
                            Self::log_message(msg_event);
                        }

                        let event = Arc::new(event);
                        if let Some(ctx) = Ctx::new(event, outgoing_tx.clone())
                            && ctx_tx.send(ctx).await.is_err() {
                                break;
                            }
                    }
                    Err(e) => {
                        warn!("Failed to parse: {}, raw: {}", e, raw);
                    }
                }
            }

            driver_handle.abort();
        });

        ctx_rx
    }
}
