use std::borrow::Cow;

use crate::core::{
    adapter::MsgContext,
    context::Context,
    model::{BotId, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
    plugin::OutboundSender,
};

#[derive(Clone)]
pub struct Ctx {
    line: String,
}

impl Ctx {
    #[must_use]
    pub const fn new(line: String) -> Self {
        Self { line }
    }

    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }

    #[must_use]
    pub fn to_event_envelope(&self) -> EventEnvelope {
        let platform = PlatformId::new("console");
        let user = UserRef::new(platform.clone(), "console-user");
        let channel = ChannelRef::direct(platform.clone(), "console-user");
        let message = MessageEvent::new(user, channel, self.text());
        EventEnvelope::new(BotId::new("console"), platform).with_message(message)
    }

    #[must_use]
    pub fn into_context(self, sender: Option<std::sync::Arc<dyn OutboundSender>>) -> Context {
        let envelope = self.to_event_envelope();
        Context::new(envelope, sender, self)
    }
}

impl MsgContext for Ctx {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.line.trim())
    }

    fn user_id(&self) -> Cow<'_, str> {
        Cow::Borrowed("console-user")
    }

    fn group_id(&self) -> Option<Cow<'_, str>> {
        None
    }
}
