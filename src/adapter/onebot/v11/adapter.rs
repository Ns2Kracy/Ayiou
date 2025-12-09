use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::adapter::onebot::v11::api::Api;
use crate::adapter::onebot::v11::model::{ApiRequest, Message, MessageEvent, OneBotEvent};
use crate::core::Driver;
use crate::driver::wsclient::{RawMessage, WsDriver};

/// OneBot v11 Adapter
///
/// Handles:
/// - Driver management (WebSocket/HTTP)
/// - Protocol parsing (raw -> OneBotEvent)
/// - API request building
pub struct OneBotV11Adapter {
    url: String,
    event_tx: Option<mpsc::Sender<OneBotEvent>>,
    outgoing_rx: Option<mpsc::Receiver<String>>,
}

impl OneBotV11Adapter {
    /// Create adapter with WebSocket URL
    pub fn ws(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            event_tx: None,
            outgoing_rx: None,
        }
    }

    /// Initialize channels and return Api instance
    pub fn connect(&mut self, event_tx: mpsc::Sender<OneBotEvent>) -> Api {
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        self.event_tx = Some(event_tx);
        self.outgoing_rx = Some(outgoing_rx);
        Api::new(outgoing_tx)
    }

    /// Run the adapter (starts driver and event processing)
    pub async fn run(mut self) -> Result<()> {
        let event_tx = self
            .event_tx
            .take()
            .ok_or_else(|| anyhow!("Not connected, call connect() first"))?;
        let outgoing_rx = self
            .outgoing_rx
            .take()
            .ok_or_else(|| anyhow!("Not connected, call connect() first"))?;

        // Create channel for driver -> adapter
        let (raw_tx, mut raw_rx) = mpsc::channel::<RawMessage>(100);

        // Create and start driver
        let driver = WsDriver::new(&self.url, raw_tx, outgoing_rx);
        let driver_handle = tokio::spawn(async move {
            if let Err(e) = Box::new(driver).run().await {
                error!("Driver error: {}", e);
            }
        });

        // Process incoming messages
        while let Some(raw) = raw_rx.recv().await {
            match Self::parse(raw) {
                Ok(Some(event)) => {
                    Self::log_event(&event);
                    if event_tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("Parse error: {}", e);
                    break;
                }
            }
        }

        driver_handle.abort();
        Ok(())
    }

    /// Parse raw message into OneBot event
    fn parse(raw: RawMessage) -> Result<Option<OneBotEvent>> {
        let text = match raw {
            RawMessage::Text(t) => t,
            RawMessage::Binary(_) => return Ok(None),
            RawMessage::Close => return Err(anyhow!("Connection closed")),
        };

        match serde_json::from_str::<OneBotEvent>(&text) {
            Ok(event) => Ok(Some(event)),
            Err(e) => {
                warn!("Failed to parse: {}, raw: {}", e, text);
                Ok(None)
            }
        }
    }

    /// Build API request JSON
    pub fn build_request(action: &str, params: serde_json::Value) -> Result<String> {
        Ok(serde_json::to_string(&ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        })?)
    }

    fn log_event(event: &OneBotEvent) {
        if let OneBotEvent::Message(msg_event) = event {
            info!("{}", Self::format_message_event(msg_event));
        }
    }

    fn format_message_event(msg_event: &MessageEvent) -> String {
        match msg_event {
            MessageEvent::Private(p) => format!(
                "收到 <- 私聊 [{}({})] {}",
                p.sender.nickname,
                p.user_id,
                Self::format_message(&p.message)
            ),
            MessageEvent::Group(g) => format!(
                "收到 <- 群聊 [{}] [{}({})] {}",
                g.group_id,
                g.sender.card.as_deref().unwrap_or(&g.sender.nickname),
                g.user_id,
                Self::format_message(&g.message)
            ),
        }
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
