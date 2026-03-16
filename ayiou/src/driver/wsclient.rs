use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, protocol::Message},
};
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

    pub fn with_access_token(url: &str, token: impl AsRef<str>) -> Self {
        let mut parsed = url::Url::parse(url).expect("Invalid WebSocket URL");
        let has_access_token = parsed.query_pairs().any(|(key, _)| key == "access_token");

        if !has_access_token && !token.as_ref().trim().is_empty() {
            parsed
                .query_pairs_mut()
                .append_pair("access_token", token.as_ref());
        }

        Self { url: parsed }
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn redacted_url(&self) -> String {
        let mut redacted = self.url.clone();
        let query_pairs: Vec<(String, String)> = redacted
            .query_pairs()
            .map(|(key, value)| {
                if key == "access_token" {
                    (key.to_string(), "***".to_string())
                } else {
                    (key.to_string(), value.to_string())
                }
            })
            .collect();

        redacted.set_query(None);
        if !query_pairs.is_empty() {
            let mut pairs = redacted.query_pairs_mut();
            for (key, value) in query_pairs {
                pairs.append_pair(&key, &value);
            }
        }

        redacted.to_string()
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
                    info!("WebSocket connected to {}", self.redacted_url());
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
                Err(e) => {
                    match &e {
                        WsError::Http(response)
                            if response.status().as_u16() == 401
                                || response.status().as_u16() == 403 =>
                        {
                            warn!(
                                "Connection to {} failed with status {} (access_token may be invalid), reconnecting in {}s...",
                                self.redacted_url(),
                                response.status(),
                                retry_delay.as_secs()
                            );
                        }
                        _ => warn!(
                            "Connection to {} failed: {}, Reconnecting in {}s...",
                            self.redacted_url(),
                            e,
                            retry_delay.as_secs()
                        ),
                    }
                }
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
