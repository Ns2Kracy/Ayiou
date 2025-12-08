use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{info, warn};
use url::Url;

/// Type-erased future
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Raw transport message
#[derive(Debug)]
pub enum RawMessage {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

/// Driver trait for transport layer abstraction
///
/// Implementors handle the actual network transport (WebSocket, HTTP, etc.)
/// without any protocol-specific logic.
pub trait Driver: Send + 'static {
    /// Start the driver and run until completion or error
    fn run(self: Box<Self>) -> BoxFuture<'static, Result<()>>;
}

/// WebSocket driver
///
/// Pure transport layer, no protocol parsing
pub struct WsDriver {
    url: Url,
    outgoing_rx: mpsc::Receiver<String>,
    incoming_tx: mpsc::Sender<RawMessage>,
}

impl WsDriver {
    pub fn new(
        url: &str,
        incoming_tx: mpsc::Sender<RawMessage>,
        outgoing_rx: mpsc::Receiver<String>,
    ) -> Self {
        Self {
            url: url::Url::parse(url).expect("Invalid WebSocket URL"),
            outgoing_rx,
            incoming_tx,
        }
    }

    async fn run_inner(mut self) -> Result<()> {
        let mut retry_delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);

        loop {
            match connect_async(self.url.as_str()).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected to {}", &self.url);
                    retry_delay = Duration::from_secs(1);

                    let (mut sink, mut stream) = ws_stream.split();

                    loop {
                        tokio::select! {
                            msg = stream.next() => {
                                match msg {
                                    Some(Ok(ws_msg)) => {
                                        if let Some(raw) = Self::convert(ws_msg) {
                                            if self.incoming_tx.send(raw).await.is_err() {
                                                return Ok(());
                                            }
                                        }
                                    }
                                    Some(Err(e)) => {
                                        warn!("WebSocket error: {}", e);
                                        break;
                                    }
                                    None => break,
                                }
                            }
                            msg = self.outgoing_rx.recv() => {
                                match msg {
                                    Some(data) => {
                                        if sink.send(WsMessage::Text(data.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    None => return Ok(()),
                                }
                            }
                        }
                    }
                }
                Err(e) => warn!("Connection failed: {}", e),
            }

            warn!("Reconnecting in {}s...", retry_delay.as_secs());
            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(max_delay);
        }
    }

    fn convert(msg: WsMessage) -> Option<RawMessage> {
        match msg {
            WsMessage::Text(t) => Some(RawMessage::Text(t.to_string())),
            WsMessage::Binary(b) => Some(RawMessage::Binary(b.to_vec())),
            WsMessage::Close(_) => Some(RawMessage::Close),
            _ => None,
        }
    }
}

impl Driver for WsDriver {
    fn run(self: Box<Self>) -> BoxFuture<'static, Result<()>> {
        Box::pin(self.run_inner())
    }
}
