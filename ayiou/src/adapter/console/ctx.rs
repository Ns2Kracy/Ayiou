use crate::core::{
    adapter::MsgContext,
    context::Context,
    model::{BotId, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
    plugin_host::OutboundSender,
};

#[derive(Clone)]
pub struct Ctx {
    line: String,
}

impl Ctx {
    pub fn new(line: String) -> Self {
        Self { line }
    }

    pub fn line(&self) -> &str {
        &self.line
    }

    pub fn to_event_envelope(&self) -> EventEnvelope {
        let platform = PlatformId::new("console");
        let user = UserRef::new(platform.clone(), "console-user");
        let channel = ChannelRef::direct(platform.clone(), "console-user");
        let message = MessageEvent::new(user, channel, self.text());
        EventEnvelope::new(BotId::new("console"), platform).with_message(message)
    }

    pub fn into_context(self, sender: Option<std::sync::Arc<dyn OutboundSender>>) -> Context {
        let envelope = self.to_event_envelope();
        Context::new(envelope, sender, self)
    }
}

impl MsgContext for Ctx {
    fn text(&self) -> String {
        self.line.trim().to_string()
    }

    fn user_id(&self) -> String {
        "console-user".to_string()
    }

    fn group_id(&self) -> Option<String> {
        None
    }
}
