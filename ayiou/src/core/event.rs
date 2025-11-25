use std::fmt::Debug;

/// 消息事件，包含平台、用户、群组、消息内容等信息
#[derive(Debug, Clone)]
pub struct Event {
    /// 事件类型 (如 "console.message", "onebot.message")
    pub name: String,
    /// 平台名称 (如 "console", "onebot_v11")
    pub platform: String,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 群组 ID (私聊时为 None)
    pub group_id: Option<String>,
    /// 消息内容
    pub message: Option<String>,
    /// 原始数据 (JSON 字符串等)
    pub raw: String,
}

impl Event {
    pub fn new(name: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            platform: platform.into(),
            user_id: None,
            group_id: None,
            message: None,
            raw: String::new(),
        }
    }

    pub fn user_id(mut self, id: impl Into<String>) -> Self {
        self.user_id = Some(id.into());
        self
    }

    pub fn group_id(mut self, id: impl Into<String>) -> Self {
        self.group_id = Some(id.into());
        self
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    pub fn raw(mut self, raw: impl Into<String>) -> Self {
        self.raw = raw.into();
        self
    }
}
