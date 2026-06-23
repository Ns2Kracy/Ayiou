use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmManifestDto {
    pub kind: String,
    pub version: Option<String>,
    pub description: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHandleOutcomeDto {
    #[serde(default)]
    pub block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHealthDto {
    pub healthy: bool,
    pub detail: Option<String>,
}
