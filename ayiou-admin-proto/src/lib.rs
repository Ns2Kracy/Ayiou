use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigBackend {
    Toml,
    Sqlite,
    Postgres,
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdminCommand {
    StartBot,
    StopBot,
    EnablePlugin {
        plugin_name: String,
    },
    DisablePlugin {
        plugin_name: String,
    },
    UpdatePluginConfig {
        plugin_name: String,
        backend: ConfigBackend,
        content: String,
        expected_version: Option<u64>,
    },
    LoadWasmPlugin {
        plugin_name: String,
        module_path: String,
    },
    UnloadWasmPlugin {
        plugin_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub command_id: String,
    pub bot_id: String,
    pub command: AdminCommand,
}

impl CommandEnvelope {
    pub fn new(
        command_id: impl Into<String>,
        bot_id: impl Into<String>,
        command: AdminCommand,
    ) -> Self {
        Self {
            command_id: command_id.into(),
            bot_id: bot_id.into(),
            command,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_envelope_json_roundtrip() {
        let msg = CommandEnvelope::new(
            "cmd-1",
            "bot-a",
            AdminCommand::EnablePlugin {
                plugin_name: "echo".into(),
            },
        );

        let json = serde_json::to_string(&msg).unwrap();
        let back: CommandEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.command_id, "cmd-1");
    }

    #[test]
    fn wasm_command_envelope_json_roundtrip() {
        let msg = CommandEnvelope::new(
            "cmd-2",
            "bot-a",
            AdminCommand::LoadWasmPlugin {
                plugin_name: "echo".into(),
                module_path: "/tmp/echo.wasm".into(),
            },
        );

        let json = serde_json::to_string(&msg).unwrap();
        let back: CommandEnvelope = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.command, AdminCommand::LoadWasmPlugin { .. }));
    }
}
