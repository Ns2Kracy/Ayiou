use crate::core::{adapter::Adapter, driver::Driver};
use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};
use url::Url;

#[derive(Debug)]
pub struct WSClientDriver {
    url: String,
    outbound_tx: mpsc::Sender<String>,
    outbound_rx: Arc<Mutex<Option<mpsc::Receiver<String>>>>,
}

impl WSClientDriver {
    pub fn new(url: &str) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            url: url.to_string(),
            outbound_tx: tx,
            outbound_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }
}

#[async_trait]
impl Driver for WSClientDriver {
    async fn run(&self, adapter: Arc<dyn Adapter>) -> Result<()> {
        let url = Url::parse(&self.url).context("Invalid WebSocket URL")?;

        let mut rx = self
            .outbound_rx
            .lock()
            .await
            .take()
            .context("Driver already started")?;

        loop {
            info!("Connecting to WebSocket: {}", url);
            match connect_async(url.as_str()).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected to {}", &self.url);
                    let (mut write, mut read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            maybe_msg = rx.recv() => {
                                match maybe_msg {
                                    Some(text) => {
                                        if let Err(e) = write.send(Message::Text(text.into())).await {
                                            error!("Failed to send message to WS: {}", e);
                                            break;
                                        }
                                    }
                                    None => {
                                        info!("Outbound channel closed, stopping driver.");
                                        return Ok(());
                                    }
                                }
                            }
                            maybe_ws_msg = read.next() => {
                                match maybe_ws_msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Err(e) = adapter.handle(text.to_string()).await {
                                            warn!("Adapter failed to handle event: {}", e);
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!("WebSocket closed by server");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        error!("WebSocket read error: {}", e);
                                        break;
                                    }
                                    None => {
                                        info!("WebSocket stream ended");
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    info!("WebSocket disconnected from {}", &self.url);
                }
                Err(e) => {
                    error!("Failed to connect to {}: {}", url, e);
                }
            }
            warn!("Reconnecting in 5 seconds...");
            sleep(Duration::from_secs(5)).await;
        }
    }

    async fn send(&self, content: String) -> Result<()> {
        self.outbound_tx
            .send(content)
            .await
            .map_err(|_| anyhow::anyhow!("WebSocket write loop closed"))
    }
}
