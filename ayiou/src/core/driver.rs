use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Driver 负责网络传输层
/// - Forward Driver: 客户端能力 (WebSocket Client, HTTP Client)
/// - Reverse Driver: 服务端能力 (HTTP Server, WebSocket Server)
#[async_trait]
pub trait Driver: Send + Sync {
    /// 启动 Driver，接收原始消息并通过 tx 发送
    async fn run(&self, tx: mpsc::Sender<String>) -> Result<()>;

    /// 发送原始消息到平台
    async fn send(&self, message: String) -> Result<()>;
}
