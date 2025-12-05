use crate::onebot::model::{GroupMessageEvent, PrivateMessageEvent};

#[derive(Clone)]
pub enum MsgEvent {
    Private(PrivateMessageEvent),
    Group(GroupMessageEvent),
}
