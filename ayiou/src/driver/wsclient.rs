use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

use crate::core::driver::Driver;

/// WebSocket driver (raw text frames).
///
/// Pure transport layer, no protocol parsing.
pub struct WsDriver {
    url: Url,
}

impl WsDriver {
    pub fn new(url: &str) -> Self {
        Self {
            url: url::Url::parse(url).expect("Invalid WebSocket URL"),
        }
    }

    async fn run_inner(
        self,
        inbound_tx: mpsc::Sender<String>,
        mut outbound_rx: mpsc::Receiver<String>,
    ) -> Result<()> {
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
                                    Some(Ok(Message::Text(text))) => {
                                        let payload = text.to_string();
                                        if inbound_tx.send(payload).await.is_err() {
                                            return Ok(());
                                        }
                                    }
                                    Some(Ok(Message::Binary(_))) => continue,
                                    Some(Ok(Message::Close(_))) => break,
                                    Some(Ok(_)) => continue,
                                    Some(Err(e)) => {
                                        warn!("WebSocket error: {}", e);
                                        break;
                                    }
                                    None => break,
                                }
                            }
                            msg = outbound_rx.recv() => {
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
}

#[async_trait::async_trait]
impl Driver for WsDriver {
    type Inbound = String;
    type Outbound = String;

    async fn run(
        self: Box<Self>,
        inbound_tx: mpsc::Sender<Self::Inbound>,
        outbound_rx: mpsc::Receiver<Self::Outbound>,
    ) -> Result<()> {
        self.run_inner(inbound_tx, outbound_rx).await
    }
}
