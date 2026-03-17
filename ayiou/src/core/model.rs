use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BotId(String);

impl BotId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for BotId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for BotId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for BotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlatformId(String);

impl PlatformId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PlatformId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for PlatformId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for PlatformId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelKind {
    Direct,
    Group,
    Channel,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelRef {
    platform: PlatformId,
    channel_id: String,
    kind: ChannelKind,
}

impl ChannelRef {
    pub fn new(
        platform: impl Into<PlatformId>,
        channel_id: impl Into<String>,
        kind: ChannelKind,
    ) -> Self {
        Self {
            platform: platform.into(),
            channel_id: channel_id.into(),
            kind,
        }
    }

    pub fn direct(platform: impl Into<PlatformId>, user_id: impl Into<String>) -> Self {
        Self::new(platform, user_id, ChannelKind::Direct)
    }

    pub fn group(platform: impl Into<PlatformId>, group_id: impl Into<String>) -> Self {
        Self::new(platform, group_id, ChannelKind::Group)
    }

    pub fn channel(platform: impl Into<PlatformId>, channel_id: impl Into<String>) -> Self {
        Self::new(platform, channel_id, ChannelKind::Channel)
    }

    pub fn platform(&self) -> &PlatformId {
        &self.platform
    }

    pub fn channel_id(&self) -> &str {
        &self.channel_id
    }

    pub fn kind(&self) -> ChannelKind {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserRef {
    platform: PlatformId,
    user_id: String,
    display_name: Option<String>,
}

impl UserRef {
    pub fn new(platform: impl Into<PlatformId>, user_id: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            user_id: user_id.into(),
            display_name: None,
        }
    }

    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    pub fn platform(&self) -> &PlatformId {
        &self.platform
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    command: String,
    args: String,
    prefix: Option<String>,
}

impl CommandInvocation {
    pub fn new(
        command: impl Into<String>,
        args: impl Into<String>,
        prefix: Option<impl Into<String>>,
    ) -> Self {
        Self {
            command: command.into(),
            args: args.into(),
            prefix: prefix.map(Into::into),
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &str {
        &self.args
    }

    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageSegment {
    Text {
        text: String,
    },
    Mention {
        user_id: String,
    },
    Image {
        url: String,
    },
    Attachment {
        name: Option<String>,
        url: Option<String>,
        mime: Option<String>,
    },
    Unknown {
        kind: String,
        data: serde_json::Value,
    },
}

impl MessageSegment {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEvent {
    pub message_id: Option<String>,
    pub sender: UserRef,
    pub channel: ChannelRef,
    pub text: String,
    pub segments: Vec<MessageSegment>,
}

impl MessageEvent {
    pub fn new(sender: UserRef, channel: ChannelRef, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            message_id: None,
            sender,
            channel,
            segments: vec![MessageSegment::text(text.clone())],
            text,
        }
    }

    pub fn with_message_id(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    pub fn with_segments(mut self, segments: Vec<MessageSegment>) -> Self {
        self.text = plain_text_from_segments(&segments);
        self.segments = segments;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub bot_id: BotId,
    pub platform: PlatformId,
    pub received_at: SystemTime,
    pub message: Option<MessageEvent>,
}

impl EventEnvelope {
    pub fn new(bot_id: impl Into<BotId>, platform: impl Into<PlatformId>) -> Self {
        Self {
            bot_id: bot_id.into(),
            platform: platform.into(),
            received_at: SystemTime::now(),
            message: None,
        }
    }

    pub fn with_message(mut self, message: MessageEvent) -> Self {
        self.message = Some(message);
        self
    }

    pub fn message(&self) -> Option<&MessageEvent> {
        self.message.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub target: ChannelRef,
    pub segments: Vec<MessageSegment>,
}

impl OutboundMessage {
    pub fn new(target: ChannelRef, segments: Vec<MessageSegment>) -> Self {
        Self { target, segments }
    }

    pub fn text(target: ChannelRef, text: impl Into<String>) -> Self {
        Self::new(target, vec![MessageSegment::text(text)])
    }

    pub fn plain_text(&self) -> String {
        plain_text_from_segments(&self.segments)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutboundReceipt {
    pub message_id: Option<String>,
}

pub fn plain_text_from_segments(segments: &[MessageSegment]) -> String {
    let mut text = String::new();

    for segment in segments {
        match segment {
            MessageSegment::Text { text: value } => text.push_str(value),
            MessageSegment::Mention { user_id } => {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push('@');
                text.push_str(user_id);
            }
            MessageSegment::Image { .. }
            | MessageSegment::Attachment { .. }
            | MessageSegment::Unknown { .. } => {}
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_message_plain_text_keeps_text_segments() {
        let msg = OutboundMessage::new(
            ChannelRef::group("onebot", "42"),
            vec![
                MessageSegment::text("hello"),
                MessageSegment::Mention {
                    user_id: "u1".to_string(),
                },
            ],
        );

        assert_eq!(msg.plain_text(), "hello @u1");
    }
}
