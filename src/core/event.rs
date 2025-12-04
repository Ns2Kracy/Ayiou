use crate::onebot::model::OneBotEvent;

/// 代表一个从 Bot 连接实例传来的事件
#[derive(Debug, Clone)]
pub struct Event {
    /// 来自 OneBot v11 协议的原始事件
    pub event: OneBotEvent,
}

impl Event {
    pub fn new(event: OneBotEvent) -> Self {
        Self { event }
    }
}
