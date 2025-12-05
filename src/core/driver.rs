use anyhow::Result;
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message as TungsteniteMessage,
};
use tracing::{info, warn};

use crate::onebot::model::{Message, MessageEvent, MessageSegment, OneBotEvent};

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, TungsteniteMessage>;

/// 代表一个到 OneBot v11 服务端的连接
pub struct Driver {
    url: url::Url,
    api_rx: mpsc::Receiver<String>,
    event_tx: mpsc::Sender<OneBotEvent>,
}

impl Driver {
    /// 创建一个新的 BotConnection
    pub fn new(
        url: String,
        event_tx: mpsc::Sender<OneBotEvent>,
        api_rx: mpsc::Receiver<String>,
    ) -> Self {
        Self {
            url: url::Url::parse(&url).expect("Invalid WebSocket URL"),
            api_rx,
            event_tx,
        }
    }

    /// 启动连接的读写循环
    pub async fn run(mut self) -> Result<()> {
        loop {
            match connect_async(self.url.as_str()).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected to {}", &self.url);
                    let (mut sink, mut read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            // 从 WebSocket 读取事件
                            Some(Ok(msg)) = read.next() => {
                                if self.handle_ws_message(msg).await.is_err() {
                                    break;
                                }
                            },
                            // 从 API Client 接收待发送的消息
                            Some(msg) = self.api_rx.recv() => {
                                if self.write_ws_message(&mut sink, msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!("WebSocket connection failed: {}", err);
                }
            }
            warn!("WebSocket disconnected, reconnecting in 5s...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    async fn handle_ws_message(&self, msg: TungsteniteMessage) -> Result<()> {
        if let TungsteniteMessage::Text(text) = msg {
            match serde_json::from_str::<OneBotEvent>(&text) {
                Ok(mut event) => {
                    if let OneBotEvent::Message(msg_event) = &mut event {
                        info!("{}", Self::format_message_event_for_log(msg_event));

                        // Process message content for plugins
                        match &mut **msg_event {
                            MessageEvent::Private(p) => {
                                Self::process_message_content(&mut p.message);
                            }
                            MessageEvent::Group(g) => {
                                Self::process_message_content(&mut g.message);
                            }
                        }
                        info!("Processed message event: {:?}", event);
                    }
                    self.event_tx.send(event).await?;
                }
                Err(e) => {
                    warn!("Failed to parse event: {}", e);
                }
            }
        }
        Ok(())
    }

    async fn write_ws_message(&self, sink: &mut WsSink, msg: String) -> Result<()> {
        sink.send(TungsteniteMessage::Text(msg.into())).await?;
        Ok(())
    }

    fn format_message_event_for_log(msg_event: &MessageEvent) -> String {
        match msg_event {
            MessageEvent::Private(p) => {
                format!(
                    "接收 <- 私聊 [{}({})] {:?}",
                    &p.sender.nickname, p.user_id, p.message
                )
            }
            MessageEvent::Group(g) => {
                let sender_name = g.sender.card.as_deref().unwrap_or(&g.sender.nickname);

                format!(
                    "接收 <- 群聊 [{}] [{}({})] {:?}",
                    g.group_id, sender_name, g.user_id, g.message
                )
            }
        }
    }

    // Helper function to process message content
    fn process_message_content(message: &mut Message) {
        if let Message::Array(arr) = message {
            let mut full_text = String::new();
            for segment in arr.drain(..) {
                if let MessageSegment::Text { text } = segment {
                    full_text.push_str(&text);
                }
            }
            *message = Message::String(full_text);
        }
    }
}
