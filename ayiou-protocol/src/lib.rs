//! Ayiou External Plugin Protocol
//!
//! This crate defines the JSON-RPC protocol for communication between
//! the Ayiou core and external plugins written in any language.
//!
//! # Protocol Overview
//!
//! Communication occurs over stdio using JSON-RPC 2.0 format.
//!
//! ## Methods
//!
//! - `metadata` - Get plugin metadata
//! - `matches` - Check if plugin handles this message
//! - `handle` - Handle an event
//! - `lifecycle` - Lifecycle events (startup/shutdown)

mod rpc;
mod types;

pub use rpc::*;
pub use types::*;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// JSON-RPC version
pub const JSONRPC_VERSION: &str = "2.0";
