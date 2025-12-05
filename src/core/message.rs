use std::sync::Arc;

use crate::onebot::model::{GroupMessageEvent, PrivateMessageEvent};

/// 消息事件（Arc 包装，零成本 clone）
#[derive(Clone)]
pub enum MsgEvent {
    Private(Arc<PrivateMessageEvent>),
    Group(Arc<GroupMessageEvent>),
}
