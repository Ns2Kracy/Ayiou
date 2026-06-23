use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPluginPackageDto {
    pub manifest: WasmManifestDto,
    #[serde(default)]
    pub handlers: Vec<WasmHandlerDto>,
    #[serde(default)]
    pub handle_outcome: WasmHandleOutcomeDto,
    #[serde(default)]
    pub health: WasmHealthDto,
}

impl WasmPluginPackageDto {
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            manifest: WasmManifestDto::new(kind),
            handlers: Vec::new(),
            handle_outcome: WasmHandleOutcomeDto::default(),
            health: WasmHealthDto::default(),
        }
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.manifest.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.manifest.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.handlers.push(WasmHandlerDto::command(command));
        self
    }

    #[must_use]
    pub fn handler(mut self, handler: WasmHandlerDto) -> Self {
        self.handlers.push(handler);
        self
    }

    #[must_use]
    pub fn block(mut self, block: bool) -> Self {
        self.handle_outcome.block = block;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmManifestDto {
    pub kind: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

impl WasmManifestDto {
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            version: None,
            description: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHandlerDto {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub regex_patterns: Vec<String>,
    #[serde(default)]
    pub wildcard: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub block: bool,
}

impl WasmHandlerDto {
    #[must_use]
    pub fn command(command: impl Into<String>) -> Self {
        Self {
            commands: vec![command.into()],
            regex_patterns: Vec::new(),
            wildcard: false,
            priority: 0,
            block: false,
        }
    }

    #[must_use]
    pub fn regex(pattern: impl Into<String>) -> Self {
        Self {
            commands: Vec::new(),
            regex_patterns: vec![pattern.into()],
            wildcard: false,
            priority: 0,
            block: false,
        }
    }

    #[must_use]
    pub const fn wildcard() -> Self {
        Self {
            commands: Vec::new(),
            regex_patterns: Vec::new(),
            wildcard: true,
            priority: 0,
            block: false,
        }
    }

    #[must_use]
    pub const fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub const fn block(mut self, block: bool) -> Self {
        self.block = block;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WasmHandleOutcomeDto {
    #[serde(default)]
    pub block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHealthDto {
    #[serde(default = "default_healthy")]
    pub healthy: bool,
    pub detail: Option<String>,
}

impl Default for WasmHealthDto {
    fn default() -> Self {
        Self {
            healthy: true,
            detail: None,
        }
    }
}

const fn default_healthy() -> bool {
    true
}
