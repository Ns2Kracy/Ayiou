use std::{any::Any, fmt::Debug, sync::Arc};

use crate::core::Context;

/// Represents a generic event from any platform (OneBot, Discord, Console, etc.)
pub trait Event: Send + Sync + Debug {
    /// The platform name (e.g., "onebot", "console", "discord")
    fn platform(&self) -> &str;

    /// The event type (e.g., "message", "notice", "request")
    fn kind(&self) -> EventKind;

    /// Unique ID of the user who triggered the event (if any)
    fn user_id(&self) -> Option<&str>;

    /// Group/Channel ID (if any)
    fn group_id(&self) -> Option<&str>;

    /// The raw message content (if applicable)
    fn message(&self) -> Option<&str>;

    /// Downcast helper.
    /// Implementation should typically be: `fn as_any(&self) -> &dyn Any { self }`
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Message,
    Notice,
    Request,
    Meta,
    Unknown,
}

/// A concrete event wrapper for basic testing/console.
#[derive(Debug, Clone)]
pub struct BaseEvent {
    pub platform: String,
    pub kind: EventKind,
    pub content: String,
    pub user_id: String,
    pub group_id: Option<String>,
}

impl Event for BaseEvent {
    fn platform(&self) -> &str {
        &self.platform
    }
    fn kind(&self) -> EventKind {
        self.kind
    }
    fn user_id(&self) -> Option<&str> {
        Some(&self.user_id)
    }
    fn group_id(&self) -> Option<&str> {
        self.group_id.as_deref()
    }
    fn message(&self) -> Option<&str> {
        Some(&self.content)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A handler handles an event.
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Check if this handler should run for this event.
    /// In a full framework, this would be handled by a Matcher system.
    fn matches(&self, _event: &dyn Event) -> bool {
        true
    }

    async fn handle(&self, ctx: Context, event: Arc<dyn Event>);
}
