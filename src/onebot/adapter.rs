use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{error, info, warn};

use crate::core::{Adapter as AdapterTrait, BoxFuture, Driver, RawMessage, WsDriver};
use crate::onebot::bot::Bot;
use crate::onebot::model::{ApiRequest, Message, MessageEvent, OneBotEvent};

/// API response
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse {
    pub status: String,
    pub retcode: i32,
    pub data: Option<serde_json::Value>,
    pub echo: Option<String>,
}

impl ApiResponse {
    pub fn is_ok(&self) -> bool {
        self.retcode == 0
    }
}

/// OneBot protocol message (event or API response)
#[derive(Debug, Clone)]
pub enum OneBotMessage {
    Event(OneBotEvent),
    Response(ApiResponse),
}

/// Outgoing API request with echo field
#[derive(Debug, Serialize)]
struct EchoRequest {
    action: String,
    params: serde_json::Value,
    echo: String,
}

/// OneBot v11 adapter
///
/// Responsibilities:
/// - Parse raw messages into OneBot events
/// - Build API requests
/// - Manage request-response matching
pub struct OneBotAdapter {
    echo_counter: AtomicU64,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<ApiResponse>>>>,
}

impl Default for OneBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OneBotAdapter {
    pub fn new() -> Self {
        Self {
            echo_counter: AtomicU64::new(0),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Parse raw message into OneBot message
    pub async fn parse(&self, raw: RawMessage) -> Result<Option<OneBotMessage>> {
        let text = match raw {
            RawMessage::Text(t) => t,
            RawMessage::Binary(_) => return Ok(None),
            RawMessage::Close => return Err(anyhow!("Connection closed")),
        };

        // Try to parse as API response
        if let Ok(response) = serde_json::from_str::<ApiResponse>(&text) {
            if let Some(ref echo) = response.echo {
                if let Some(sender) = self.pending_requests.lock().await.remove(echo) {
                    let _ = sender.send(response.clone());
                }
                return Ok(Some(OneBotMessage::Response(response)));
            }
        }

        // Parse as event
        match serde_json::from_str::<OneBotEvent>(&text) {
            Ok(event) => {
                self.log_event(&event);
                Ok(Some(OneBotMessage::Event(event)))
            }
            Err(e) => {
                warn!("Failed to parse: {}, raw: {}", e, text);
                Ok(None)
            }
        }
    }

    /// Build API request without waiting for response
    pub fn build_request(&self, action: &str, params: serde_json::Value) -> Result<String> {
        Ok(serde_json::to_string(&ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        })?)
    }

    /// Build API request and wait for response
    pub async fn build_request_with_echo(
        &self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<(String, oneshot::Receiver<ApiResponse>)> {
        let echo = format!(
            "ayiou_{}",
            self.echo_counter.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = oneshot::channel();

        self.pending_requests.lock().await.insert(echo.clone(), tx);

        let json = serde_json::to_string(&EchoRequest {
            action: action.to_string(),
            params,
            echo,
        })?;

        Ok((json, rx))
    }

    fn log_event(&self, event: &OneBotEvent) {
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

/// Driver factory function type
type DriverFactory =
    Box<dyn FnOnce(mpsc::Sender<RawMessage>, mpsc::Receiver<String>) -> Box<dyn Driver> + Send>;

/// OneBot v11 adapter builder
///
/// Use this to configure and build a OneBot connection:
/// ```ignore
/// OneBotAdapter::new()
///     .driver(WsDriver::new("ws://..."))
/// ```
pub struct OneBotAdapterBuilder {
    driver_factory: Option<DriverFactory>,
}

impl OneBotAdapterBuilder {
    /// Create a new OneBot adapter builder
    pub fn new() -> Self {
        Self {
            driver_factory: None,
        }
    }

    /// Set the driver (e.g., WsDriver, HttpDriver)
    pub fn driver<D: Driver + 'static>(mut self, url: impl Into<String>) -> Self
    where
        D: Driver,
    {
        let url = url.into();
        self.driver_factory = Some(Box::new(move |raw_tx, outgoing_rx| {
            Box::new(WsDriver::new(&url, raw_tx, outgoing_rx))
        }));
        self
    }

    /// Set WebSocket driver with URL
    pub fn ws(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        self.driver_factory = Some(Box::new(move |raw_tx, outgoing_rx| {
            Box::new(WsDriver::new(&url, raw_tx, outgoing_rx))
        }));
        self
    }

    /// Build the connection (called internally by AyiouBot)
    pub fn build(self, event_tx: mpsc::Sender<OneBotMessage>) -> OneBotConnection {
        let protocol = Arc::new(OneBotAdapter::new());
        // Create a placeholder channel - will be replaced in run()
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(100);
        let bot = Bot::new(protocol.clone(), outgoing_tx);

        OneBotConnection {
            driver_factory: self
                .driver_factory
                .expect("Driver not set, call .ws() or .driver()"),
            protocol,
            bot,
            outgoing_rx: Some(outgoing_rx),
            event_tx,
        }
    }
}

impl Default for OneBotAdapterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// OneBot v11 connection (adapter + driver)
///
/// Created via `OneBotAdapterBuilder`.
pub struct OneBotConnection {
    driver_factory: DriverFactory,
    protocol: Arc<OneBotAdapter>,
    bot: Bot,
    outgoing_rx: Option<mpsc::Receiver<String>>,
    event_tx: mpsc::Sender<OneBotMessage>,
}

impl OneBotConnection {
    async fn run_inner(mut self) -> Result<()> {
        // Create channel for driver -> adapter
        let (raw_tx, mut raw_rx) = mpsc::channel::<RawMessage>(100);

        // Use the outgoing_rx created during build (bot already has the tx side)
        let outgoing_rx = self.outgoing_rx.take().expect("outgoing_rx already taken");

        // Create and start driver
        let driver = (self.driver_factory)(raw_tx, outgoing_rx);
        let driver_handle = tokio::spawn(async move {
            if let Err(e) = driver.run().await {
                error!("Driver error: {}", e);
            }
        });

        // Process incoming messages
        let protocol = self.protocol;
        let event_tx = self.event_tx;
        while let Some(raw) = raw_rx.recv().await {
            match protocol.parse(raw).await {
                Ok(Some(OneBotMessage::Event(event))) => {
                    if event_tx.send(OneBotMessage::Event(event)).await.is_err() {
                        break;
                    }
                }
                Ok(Some(OneBotMessage::Response(_))) => {
                    // Response handled internally by protocol
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
}

impl AdapterTrait for OneBotConnection {
    type Bot = Bot;

    fn name(&self) -> &'static str {
        "onebot"
    }

    fn bot(&self) -> Self::Bot {
        self.bot.clone()
    }

    fn run(self: Box<Self>) -> BoxFuture<'static, Result<()>> {
        Box::pin(self.run_inner())
    }
}
