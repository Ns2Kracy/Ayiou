use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};
use url::Url;

use crate::core::Driver;

/// WebSocket connection timeout duration
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// WebSocket driver
///
/// Pure transport layer, no protocol parsing
pub struct WsDriver {
    url: Url,
    outgoing_rx: mpsc::Receiver<String>,
    incoming_tx: mpsc::Sender<String>,
}

impl WsDriver {
    pub fn new(
        url: &str,
        incoming_tx: mpsc::Sender<String>,
        outgoing_rx: mpsc::Receiver<String>,
    ) -> Result<Self> {
        Ok(Self {
            url: url::Url::parse(url)
                .map_err(|e| anyhow!("Invalid WebSocket URL '{}': {}", url, e))?,
            outgoing_rx,
            incoming_tx,
        })
    }

    async fn run_inner(mut self) -> Result<()> {
        let mut retry_delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);

        loop {
            match timeout(CONNECT_TIMEOUT, connect_async(self.url.as_str())).await {
                Ok(Ok((ws_stream, _))) => {
                    info!("WebSocket connected to {}", &self.url);
                    retry_delay = Duration::from_secs(1);

                    let (mut sink, mut stream) = ws_stream.split();

                    loop {
                        tokio::select! {
                            msg = stream.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        let payload = text.to_string();
                                        if self.incoming_tx.send(payload).await.is_err() {
                                            return Ok(());
                                        }
                                    }
                                    Some(Ok(Message::Binary(_))) => {
                                        // ignore binary frames for OneBot
                                        continue;
                                    }
                                    Some(Ok(Message::Close(_))) => break,
                                    Some(Ok(_)) => continue,
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
                Ok(Err(e)) => warn!(
                    "Connection failed: {}, Reconnecting in {}s...",
                    e,
                    retry_delay.as_secs()
                ),
                Err(_) => warn!(
                    "Connection timed out after {:?}, Reconnecting in {}s...",
                    CONNECT_TIMEOUT,
                    retry_delay.as_secs()
                ),
            }

            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(max_delay);
        }
    }
}

#[async_trait::async_trait]
impl Driver for WsDriver {
    async fn run(self: Box<Self>) -> Result<()> {
        self.run_inner().await
    }
}
