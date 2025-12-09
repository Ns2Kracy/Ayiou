use std::time::Duration;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};
use url::Url;

use crate::core::Driver;

/// Raw transport message
#[derive(Debug)]
pub enum RawMessage {
    Text(String),
    Binary(Vec<u8>),
    Close,
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
                                        if sink.send(Message::Text(data.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    None => return Ok(()),
                                }
                            }
                        }
                    }
                }
                Err(e) => warn!(
                    "Connection failed: {}, Reconnecting in {}s...",
                    e,
                    retry_delay.as_secs()
                ),
            }

            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(max_delay);
        }
    }

    fn convert(msg: Message) -> Option<RawMessage> {
        match msg {
            Message::Text(t) => Some(RawMessage::Text(t.to_string())),
            Message::Binary(b) => Some(RawMessage::Binary(b.to_vec())),
            Message::Close(_) => Some(RawMessage::Close),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl Driver for WsDriver {
    async fn run(self: Box<Self>) -> Result<()> {
        self.run_inner().await
    }
}
