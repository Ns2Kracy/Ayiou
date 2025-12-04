use crate::core::event::Event;
use crate::onebot::api::Api;
use crate::onebot::model::{
    GroupMessageEvent, Message, MessageEvent, MessageSegment, OneBotEvent, PrivateMessageEvent,
};
use anyhow::Result;
use std::future::Future;
use std::sync::Arc;

/// 消息上下文
#[derive(Clone)]
pub struct Ctx {
    pub api: Api,
    event: Arc<Event>,
    msg: MsgEvent,
}

#[derive(Clone)]
enum MsgEvent {
    Private(PrivateMessageEvent),
    Group(GroupMessageEvent),
}

impl Ctx {
    pub(crate) fn from_event(event: Arc<Event>, api: Api) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = &event.event else {
            return None;
        };

        let msg = match &**msg_event {
            MessageEvent::Private(p) => MsgEvent::Private(p.clone()),
            MessageEvent::Group(g) => MsgEvent::Group(g.clone()),
        };

        Some(Self { api, event, msg })
    }

    /// 消息文本
    pub fn text(&self) -> String {
        let message = match &self.msg {
            MsgEvent::Private(p) => &p.message,
            MsgEvent::Group(g) => &g.message,
        };

        match message {
            Message::String(s) => s.trim().to_string(),
            Message::Array(segments) => segments
                .iter()
                .filter_map(|seg| {
                    if let MessageSegment::Text { text } = seg {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string(),
        }
    }

    /// 原始消息
    pub fn raw_message(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.raw_message,
            MsgEvent::Group(g) => &g.raw_message,
        }
    }

    /// 发送者 ID
    pub fn user_id(&self) -> i64 {
        match &self.msg {
            MsgEvent::Private(p) => p.user_id,
            MsgEvent::Group(g) => g.user_id,
        }
    }

    /// 群 ID
    pub fn group_id(&self) -> Option<i64> {
        match &self.msg {
            MsgEvent::Private(_) => None,
            MsgEvent::Group(g) => Some(g.group_id),
        }
    }

    /// 是否私聊
    pub fn is_private(&self) -> bool {
        matches!(self.msg, MsgEvent::Private(_))
    }

    /// 是否群聊
    pub fn is_group(&self) -> bool {
        matches!(self.msg, MsgEvent::Group(_))
    }

    /// 发送者昵称
    pub fn nickname(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.sender.nickname,
            MsgEvent::Group(g) => &g.sender.nickname,
        }
    }

    /// 回复消息
    pub async fn reply(&self, message: impl Into<Message>) -> Result<()> {
        let msg = message.into();
        match &self.msg {
            MsgEvent::Private(p) => self.api.send_private_msg(p.user_id, &msg).await?,
            MsgEvent::Group(g) => self.api.send_group_msg(g.group_id, &msg).await?,
        };
        Ok(())
    }

    /// 回复文本
    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.reply(Message::String(text.into())).await
    }

    /// 原始事件
    pub fn event(&self) -> &Event {
        &self.event
    }
}

// ============================================================================
// 函数式处理器
// ============================================================================

/// 处理器 trait - 核心抽象
pub trait Handler: Send + Sync + 'static {
    fn call(&self, ctx: Ctx) -> BoxFuture<Result<bool>>;
}

pub type BoxFuture<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send>>;
pub type BoxHandler = Box<dyn Handler>;

/// 为闭包实现 Handler
impl<F, Fut> Handler for F
where
    F: Fn(Ctx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<bool>> + Send + 'static,
{
    fn call(&self, ctx: Ctx) -> BoxFuture<Result<bool>> {
        Box::pin(self(ctx))
    }
}

/// 匹配器 trait - 用于过滤
pub trait Matcher: Send + Sync + 'static {
    fn matches(&self, ctx: &Ctx) -> bool;
}

/// 命令匹配器
pub struct CommandMatcher(pub String);

impl Matcher for CommandMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.text().starts_with(&self.0)
    }
}

/// 正则匹配器
pub struct RegexMatcher(pub regex::Regex);

impl Matcher for RegexMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        self.0.is_match(&ctx.text())
    }
}

/// 组合匹配器 - And
pub struct AndMatcher<A, B>(pub A, pub B);

impl<A: Matcher, B: Matcher> Matcher for AndMatcher<A, B> {
    fn matches(&self, ctx: &Ctx) -> bool {
        self.0.matches(ctx) && self.1.matches(ctx)
    }
}

/// 组合匹配器 - Or
pub struct OrMatcher<A, B>(pub A, pub B);

impl<A: Matcher, B: Matcher> Matcher for OrMatcher<A, B> {
    fn matches(&self, ctx: &Ctx) -> bool {
        self.0.matches(ctx) || self.1.matches(ctx)
    }
}

/// 组合匹配器 - Not
pub struct NotMatcher<M>(pub M);

impl<M: Matcher> Matcher for NotMatcher<M> {
    fn matches(&self, ctx: &Ctx) -> bool {
        !self.0.matches(ctx)
    }
}

/// 私聊匹配器
pub struct PrivateMatcher;

impl Matcher for PrivateMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.is_private()
    }
}

/// 群聊匹配器
pub struct GroupMatcher;

impl Matcher for GroupMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.is_group()
    }
}

/// 用户匹配器
pub struct UserMatcher(pub i64);

impl Matcher for UserMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.user_id() == self.0
    }
}

/// 群匹配器
pub struct GroupIdMatcher(pub i64);

impl Matcher for GroupIdMatcher {
    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.group_id() == Some(self.0)
    }
}

// ============================================================================
// 消息处理器 - 组合 Matcher + Handler
// ============================================================================

pub struct MessageHandler {
    pub name: String,
    matcher: Option<Box<dyn Matcher>>,
    handler: BoxHandler,
}

impl MessageHandler {
    pub fn new<H: Handler>(name: impl Into<String>, handler: H) -> Self {
        Self {
            name: name.into(),
            matcher: None,
            handler: Box::new(handler),
        }
    }

    pub fn with_matcher<M: Matcher>(mut self, matcher: M) -> Self {
        self.matcher = Some(Box::new(matcher));
        self
    }

    pub fn matches(&self, ctx: &Ctx) -> bool {
        self.matcher.as_ref().map_or(true, |m| m.matches(ctx))
    }

    pub async fn call(&self, ctx: Ctx) -> Result<bool> {
        self.handler.call(ctx).await
    }
}

// ============================================================================
// 便捷函数
// ============================================================================

/// 创建命令匹配器
pub fn command(cmd: impl Into<String>) -> CommandMatcher {
    CommandMatcher(cmd.into())
}

/// 创建正则匹配器
pub fn regex(pattern: &str) -> RegexMatcher {
    RegexMatcher(regex::Regex::new(pattern).expect("Invalid regex"))
}

/// 私聊匹配器
pub fn private() -> PrivateMatcher {
    PrivateMatcher
}

/// 群聊匹配器
pub fn group() -> GroupMatcher {
    GroupMatcher
}

/// 用户匹配器
pub fn user(id: i64) -> UserMatcher {
    UserMatcher(id)
}

/// 群匹配器
pub fn group_id(id: i64) -> GroupIdMatcher {
    GroupIdMatcher(id)
}

/// Matcher 扩展方法
pub trait MatcherExt: Matcher + Sized {
    fn and<M: Matcher>(self, other: M) -> AndMatcher<Self, M> {
        AndMatcher(self, other)
    }

    fn or<M: Matcher>(self, other: M) -> OrMatcher<Self, M> {
        OrMatcher(self, other)
    }

    fn not(self) -> NotMatcher<Self> {
        NotMatcher(self)
    }
}

impl<T: Matcher + Sized> MatcherExt for T {}
