//! Protocol message types for external plugin communication

use serde::{Deserialize, Serialize};

// ============================================================================
// Metadata
// ============================================================================

/// Plugin metadata response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub commands: Vec<CommandInfo>,
}

/// Command information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

// ============================================================================
// Matches
// ============================================================================

/// Parameters for the matches method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchesParams {
    pub text: String,
    pub message_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
}

/// Result for the matches method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchesResult {
    pub matches: bool,
}

// ============================================================================
// Handle
// ============================================================================

/// Parameters for the handle method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleParams {
    pub message_type: String,
    pub user_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    pub text: String,
    pub raw_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_id: Option<i64>,
}

/// Result for the handle method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleResult {
    /// Whether the event was handled
    pub handled: bool,
    /// Whether to block subsequent handlers
    #[serde(default)]
    pub block: bool,
    /// Optional text reply
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// Optional structured actions
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// Actions that can be requested by external plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    /// Send a text reply
    #[serde(rename = "reply")]
    Reply { text: String },
    /// Send an image
    #[serde(rename = "image")]
    Image { url: String },
    /// Send to a specific target
    #[serde(rename = "send")]
    Send {
        target_type: String,
        target_id: i64,
        message: String,
    },
}

// ============================================================================
// Lifecycle
// ============================================================================

/// Lifecycle event parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleParams {
    pub event: LifecycleEvent,
}

/// Lifecycle event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleEvent {
    Startup,
    Shutdown,
    BotConnect { self_id: i64 },
}

/// Lifecycle result (acknowledgment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResult {
    pub ok: bool,
}

// ============================================================================
// Helpers
// ============================================================================

impl PluginMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: "0.1.0".to_string(),
            author: None,
            commands: Vec::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn version(mut self, ver: impl Into<String>) -> Self {
        self.version = ver.into();
        self
    }
}

impl HandleResult {
    /// No action taken
    pub fn ignored() -> Self {
        Self {
            handled: false,
            block: false,
            reply: None,
            actions: Vec::new(),
        }
    }

    /// Handled with a text reply
    pub fn reply(text: impl Into<String>) -> Self {
        Self {
            handled: true,
            block: true,
            reply: Some(text.into()),
            actions: Vec::new(),
        }
    }

    /// Handled without response
    pub fn handled() -> Self {
        Self {
            handled: true,
            block: true,
            reply: None,
            actions: Vec::new(),
        }
    }
}
