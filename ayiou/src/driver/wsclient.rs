use crate::core::Driver;
use anyhow::Result;
use futures::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tracing::{info, warn};
use url::Url;

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct WsClient {
    url: Url,
    sink: Arc<Mutex<Option<SplitSink<WsStream, Message>>>>,
}

impl WsClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: Url::parse(url).unwrap(),
            sink: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl Driver for WsClient {
    async fn run(&self, tx: mpsc::Sender<String>) -> Result<()> {
        loop {
            match connect_async(self.url.as_str()).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected to {}", &self.url);
                    let (sink, mut read) = ws_stream.split();
                    {
                        let mut s = self.sink.lock().await;
                        *s = Some(sink);
                    }
                    while let Some(Ok(msg)) = read.next().await {
                        if let Message::Text(text) = msg
                            && let Err(err) = tx.send(text.to_string()).await
                        {
                            warn!("Failed to send message: {}", err);
                        }
                    }
                    {
                        let mut s = self.sink.lock().await;
                        *s = None;
                    }
                }
                Err(err) => {
                    warn!("WebSocket connection failed: {}", err);
                }
            }
            warn!("WebSocket disconnected, reconnecting in 5s...");
            sleep(Duration::from_secs(5)).await;
        }
    }

    async fn send(&self, message: String) -> Result<()> {
        let mut s = self.sink.lock().await;
        if let Some(sink) = s.as_mut() {
            sink.send(Message::Text(message.into())).await?;
        }
        Ok(())
    }
}
