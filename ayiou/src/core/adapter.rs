use crate::core::Event;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Adapter 负责协议适配层
/// - 将平台原始数据转换为统一的 Event
/// - 将统一的 API 调用转换为平台请求
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Adapter 名称
    fn name(&self) -> &'static str;

    /// 将原始消息解析为 Event
    fn parse(&self, raw: &str) -> Option<Event>;

    /// 设置发送通道 (由 Bot 调用)
    fn set_sender(&self, _tx: mpsc::Sender<String>) {}

    /// 格式化并发送消息
    async fn send(&self, _target: &str, _message: &str) -> Result<()> {
        Ok(())
    }
}
