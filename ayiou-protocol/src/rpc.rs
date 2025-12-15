//! JSON-RPC message types

use serde::{Deserialize, Serialize};

/// JSON-RPC Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: u64,
}

impl RpcRequest {
    /// Create a new RPC request
    pub fn new(method: impl Into<String>, params: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id,
        }
    }
}

/// JSON-RPC Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: u64,
}

impl RpcResponse {
    /// Create a success response
    pub fn success(result: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Create an error response
    pub fn error(error: RpcError, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

/// JSON-RPC Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Standard error: Parse error
    pub fn parse_error() -> Self {
        Self::new(-32700, "Parse error")
    }

    /// Standard error: Invalid request
    pub fn invalid_request() -> Self {
        Self::new(-32600, "Invalid Request")
    }

    /// Standard error: Method not found
    pub fn method_not_found() -> Self {
        Self::new(-32601, "Method not found")
    }

    /// Standard error: Invalid params
    pub fn invalid_params() -> Self {
        Self::new(-32602, "Invalid params")
    }

    /// Standard error: Internal error
    pub fn internal_error() -> Self {
        Self::new(-32603, "Internal error")
    }
}

/// RPC Method names
pub mod methods {
    /// Get plugin metadata
    pub const METADATA: &str = "metadata";
    /// Check if plugin matches the event
    pub const MATCHES: &str = "matches";
    /// Handle an event
    pub const HANDLE: &str = "handle";
    /// Lifecycle events
    pub const LIFECYCLE: &str = "lifecycle";
}
