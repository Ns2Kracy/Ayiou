use serde::{Deserialize, Serialize};

pub mod abi {
    pub const MEMORY_EXPORT: &str = "memory";
    pub const ALLOC_EXPORT: &str = "ayiou_alloc";
    pub const ON_COMMAND_EXPORT: &str = "ayiou_on_command";
    pub const ON_REGEX_EXPORT: &str = "ayiou_on_regex";
    pub const ON_CRON_EXPORT: &str = "ayiou_on_cron";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchEvent {
    Command { command: String, args: String },
    Regex { text: String },
    Cron { expr: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCall {
    pub plugin: String,
    pub event: DispatchEvent,
}

impl HostCall {
    pub fn command(
        plugin: impl Into<String>,
        command: impl Into<String>,
        args: impl Into<String>,
    ) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Command {
                command: command.into(),
                args: args.into(),
            },
        }
    }

    pub fn regex(plugin: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Regex { text: text.into() },
        }
    }

    pub fn cron(plugin: impl Into<String>, expr: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Cron { expr: expr.into() },
        }
    }
}
