//! WASM plugin runtime using Wasmtime.
//!
//! This module provides a sandboxed execution environment for WASM plugins.
//! Plugins communicate with the host through a simple ABI.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use tracing::{debug, info};
use wasmtime::*;

use crate::adapter::onebot::v11::ctx::Ctx;
use crate::core::plugin::{Plugin, PluginMetadata};

// ============================================================================
// WASM Plugin ABI
// ============================================================================

/// Host functions exposed to WASM plugins
///
/// The ABI uses a simple JSON-based protocol:
/// - Host passes context as JSON string to guest
/// - Guest returns response as JSON string
///
/// Guest exports:
/// - `ayiou_meta() -> ptr` - Returns plugin metadata JSON
/// - `ayiou_matches(ctx_ptr, ctx_len) -> i32` - Check if plugin matches (1=yes, 0=no)
/// - `ayiou_handle(ctx_ptr, ctx_len) -> ptr` - Handle event, returns response JSON
/// - `ayiou_alloc(size) -> ptr` - Allocate memory in guest
/// - `ayiou_free(ptr)` - Free memory in guest
///
/// Host imports (ayiou namespace):
/// - `host_log(level, ptr, len)` - Log a message
/// - `host_reply(ptr, len)` - Send reply message
/// - `host_get_state(key_ptr, key_len) -> ptr` - Get saved state
/// - `host_set_state(key_ptr, key_len, val_ptr, val_len)` - Save state

// ============================================================================
// WASM Host State
// ============================================================================

/// State shared between host and WASM instance
#[derive(Default)]
pub struct WasmHostState {
    /// Pending reply message (set by guest, consumed by host)
    pub pending_reply: Option<String>,
    /// Plugin state storage (persisted across reloads)
    pub state: std::collections::HashMap<String, serde_json::Value>,
    /// Current context JSON (for guest to read)
    pub current_ctx: Option<String>,
}

// ============================================================================
// WASM Plugin Runtime
// ============================================================================

/// WASM plugin runtime engine (shared across all WASM plugins)
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    /// Create a new WASM runtime with optimized settings for bot plugins
    pub fn new() -> Result<Self> {
        let mut config = Config::new();

        // Optimize for low memory footprint (bot doesn't need max perf)
        config.cranelift_opt_level(OptLevel::Speed);
        config.wasm_bulk_memory(true);
        config.wasm_multi_value(true);

        // Disable async support - we use sync instantiation
        // (async_support requires using async instantiate methods)
        config.async_support(false);

        let engine = Engine::new(&config)?;

        Ok(Self { engine })
    }

    /// Load a WASM plugin from file
    pub async fn load_plugin(&self, path: &Path) -> Result<WasmPlugin> {
        let wasm_bytes = tokio::fs::read(path)
            .await
            .context("Failed to read WASM file")?;

        self.load_plugin_from_bytes(&wasm_bytes, path.to_path_buf())
            .await
    }

    /// Load a WASM plugin from bytes
    pub async fn load_plugin_from_bytes(
        &self,
        wasm_bytes: &[u8],
        source_path: std::path::PathBuf,
    ) -> Result<WasmPlugin> {
        let module =
            Module::new(&self.engine, wasm_bytes).context("Failed to compile WASM module")?;

        info!("Loaded WASM module from {:?}", source_path);

        WasmPlugin::new(self.engine.clone(), module, source_path)
    }

    /// Get the engine reference
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create WASM runtime")
    }
}

// ============================================================================
// WASM Plugin Instance
// ============================================================================

/// A loaded WASM plugin instance
pub struct WasmPlugin {
    engine: Engine,
    module: Module,
    source_path: std::path::PathBuf,
    metadata: PluginMetadata,
    /// Shared state (protected by mutex for thread safety)
    host_state: Arc<Mutex<WasmHostState>>,
}

impl WasmPlugin {
    fn new(engine: Engine, module: Module, source_path: std::path::PathBuf) -> Result<Self> {
        let host_state = Arc::new(Mutex::new(WasmHostState::default()));

        // Create a temporary store to get metadata
        let metadata = Self::extract_metadata(&engine, &module, host_state.clone())?;

        Ok(Self {
            engine,
            module,
            source_path,
            metadata,
            host_state,
        })
    }

    /// Extract plugin metadata by calling ayiou_meta export
    fn extract_metadata(
        engine: &Engine,
        module: &Module,
        host_state: Arc<Mutex<WasmHostState>>,
    ) -> Result<PluginMetadata> {
        let mut store = Store::new(engine, host_state);
        let linker = Self::create_linker(engine)?;

        let instance = linker
            .instantiate(&mut store, module)
            .context("Failed to instantiate WASM module")?;

        // Try to call ayiou_meta
        let meta_fn = instance
            .get_typed_func::<(), i32>(&mut store, "ayiou_meta")
            .context("WASM plugin missing 'ayiou_meta' export")?;

        let ptr = meta_fn.call(&mut store, ())?;

        // Read the metadata JSON from guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("WASM plugin missing 'memory' export")?;

        let json_str = Self::read_guest_string(&store, &memory, ptr)?;
        let meta: WasmPluginMeta =
            serde_json::from_str(&json_str).context("Failed to parse plugin metadata JSON")?;

        Ok(PluginMetadata::new(&meta.name)
            .description(&meta.description)
            .version(&meta.version))
    }

    /// Create linker with host functions
    fn create_linker(engine: &Engine) -> Result<Linker<Arc<Mutex<WasmHostState>>>> {
        let mut linker = Linker::new(engine);

        // ====================================================================
        // AssemblyScript runtime imports (env namespace)
        // ====================================================================

        // env::abort(message: i32, fileName: i32, lineNumber: i32, columnNumber: i32)
        // Called when AssemblyScript encounters an assertion failure or abort
        linker.func_wrap(
            "env",
            "abort",
            |mut caller: Caller<'_, Arc<Mutex<WasmHostState>>>,
             msg_ptr: i32,
             file_ptr: i32,
             line: i32,
             col: i32| {
                // Try to read error message from memory (AssemblyScript uses UTF-16)
                let error_msg = if let Some(memory) =
                    caller.get_export("memory").and_then(|e| e.into_memory())
                {
                    let data = memory.data(&caller);
                    // AssemblyScript strings are length-prefixed UTF-16
                    let ptr = msg_ptr as usize;
                    if ptr > 0 && ptr + 4 <= data.len() {
                        let len = u32::from_le_bytes([
                            data[ptr],
                            data[ptr + 1],
                            data[ptr + 2],
                            data[ptr + 3],
                        ]) as usize;
                        let str_start = ptr + 4;
                        let str_end = str_start + len;
                        if str_end <= data.len() {
                            // Convert UTF-16 to String
                            let utf16: Vec<u16> = data[str_start..str_end]
                                .chunks(2)
                                .filter_map(|c| {
                                    if c.len() == 2 {
                                        Some(u16::from_le_bytes([c[0], c[1]]))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            String::from_utf16_lossy(&utf16)
                        } else {
                            "<invalid>".to_string()
                        }
                    } else {
                        "<null>".to_string()
                    }
                } else {
                    "<unknown>".to_string()
                };
                let _ = file_ptr; // suppress unused warning

                tracing::error!("WASM abort: {} (at line {}, col {})", error_msg, line, col);
            },
        )?;

        // ====================================================================
        // Ayiou plugin imports (ayiou namespace)
        // ====================================================================

        // host_log(level: i32, ptr: i32, len: i32)
        linker.func_wrap(
            "ayiou",
            "host_log",
            |_caller: Caller<'_, Arc<Mutex<WasmHostState>>>, level: i32, _ptr: i32, _len: i32| {
                // TODO: Read string from guest memory and log
                debug!("WASM plugin log (level {})", level);
            },
        )?;

        // host_reply(ptr: i32, len: i32)
        linker.func_wrap(
            "ayiou",
            "host_reply",
            |mut caller: Caller<'_, Arc<Mutex<WasmHostState>>>, ptr: i32, len: i32| {
                if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                    let data = memory.data(&caller);
                    let start = ptr as usize;
                    let end = start + len as usize;
                    if end <= data.len()
                        && let Ok(msg) = std::str::from_utf8(&data[start..end])
                    {
                        let state = caller.data();
                        state.lock().pending_reply = Some(msg.to_string());
                    }
                }
            },
        )?;

        Ok(linker)
    }

    /// Read a null-terminated or length-prefixed string from guest memory
    fn read_guest_string(
        store: &Store<Arc<Mutex<WasmHostState>>>,
        memory: &Memory,
        ptr: i32,
    ) -> Result<String> {
        let data = memory.data(store);
        let start = ptr as usize;

        // Read length prefix (first 4 bytes as little-endian u32)
        if start + 4 > data.len() {
            anyhow::bail!("Invalid string pointer");
        }

        let len = u32::from_le_bytes([
            data[start],
            data[start + 1],
            data[start + 2],
            data[start + 3],
        ]) as usize;

        let str_start = start + 4;
        let str_end = str_start + len;

        if str_end > data.len() {
            anyhow::bail!("String extends beyond memory bounds");
        }

        String::from_utf8(data[str_start..str_end].to_vec())
            .context("Invalid UTF-8 in guest string")
    }

    /// Write a string to guest memory, returns pointer
    fn write_guest_string(
        store: &mut Store<Arc<Mutex<WasmHostState>>>,
        memory: &Memory,
        instance: &Instance,
        s: &str,
    ) -> Result<i32> {
        let bytes = s.as_bytes();
        let total_len = 4 + bytes.len(); // length prefix + data

        // Call guest's alloc function
        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut *store, "ayiou_alloc")
            .context("WASM plugin missing 'ayiou_alloc' export")?;

        let ptr = alloc_fn.call(&mut *store, total_len as i32)?;

        // Write length prefix and data
        let data = memory.data_mut(&mut *store);
        let start = ptr as usize;

        data[start..start + 4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        data[start + 4..start + 4 + bytes.len()].copy_from_slice(bytes);

        Ok(ptr)
    }

    /// Get source path
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }
}

/// Metadata structure expected from WASM plugins
#[derive(serde::Deserialize)]
struct WasmPluginMeta {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_version")]
    version: String,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// Context data passed to WASM plugins
#[derive(serde::Serialize)]
struct WasmContext {
    text: String,
    raw_message: String,
    user_id: i64,
    group_id: Option<i64>,
    is_private: bool,
    is_group: bool,
    nickname: String,
}

impl From<&Ctx> for WasmContext {
    fn from(ctx: &Ctx) -> Self {
        Self {
            text: ctx.text(),
            raw_message: ctx.raw_message().to_string(),
            user_id: ctx.user_id(),
            group_id: ctx.group_id(),
            is_private: ctx.is_private(),
            is_group: ctx.is_group(),
            nickname: ctx.nickname().to_string(),
        }
    }
}

/// Response from WASM plugin handle function
#[derive(serde::Deserialize)]
struct WasmHandleResponse {
    /// Whether to block subsequent handlers
    block: bool,
    /// Optional reply message
    reply: Option<String>,
}

// ============================================================================
// Plugin trait implementation
// ============================================================================

#[async_trait::async_trait]
impl Plugin for WasmPlugin {
    fn meta(&self) -> PluginMetadata {
        self.metadata.clone()
    }

    fn matches(&self, ctx: &Ctx) -> bool {
        // Create a fresh store for this call
        let mut store = Store::new(&self.engine, self.host_state.clone());
        let linker = match Self::create_linker(&self.engine) {
            Ok(l) => l,
            Err(_) => return false,
        };

        let instance = match linker.instantiate(&mut store, &self.module) {
            Ok(i) => i,
            Err(_) => return false,
        };

        let memory = match instance.get_memory(&mut store, "memory") {
            Some(m) => m,
            None => return false,
        };

        // Serialize context to JSON
        let ctx_json = serde_json::to_string(&WasmContext::from(ctx)).unwrap_or_default();

        // Write context to guest memory
        let ctx_ptr = match Self::write_guest_string(&mut store, &memory, &instance, &ctx_json) {
            Ok(p) => p,
            Err(_) => return false,
        };

        // Call ayiou_matches
        let matches_fn =
            match instance.get_typed_func::<(i32, i32), i32>(&mut store, "ayiou_matches") {
                Ok(f) => f,
                Err(_) => return true, // Default to matching if function not found
            };

        matches_fn
            .call(&mut store, (ctx_ptr, ctx_json.len() as i32))
            .map(|r| r != 0)
            .unwrap_or(false)
    }

    async fn handle(&self, ctx: &Ctx) -> Result<bool> {
        // Clear any pending reply from previous calls
        {
            let mut state = self.host_state.lock();
            state.pending_reply = None;
        }

        // Create a fresh store for this call
        let mut store = Store::new(&self.engine, self.host_state.clone());
        let linker = Self::create_linker(&self.engine)?;

        let instance = linker.instantiate(&mut store, &self.module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .context("WASM plugin missing 'memory' export")?;

        // Serialize context to JSON
        let ctx_json = serde_json::to_string(&WasmContext::from(ctx))?;

        // Write context to guest memory
        let ctx_ptr = Self::write_guest_string(&mut store, &memory, &instance, &ctx_json)?;

        // Call ayiou_handle
        let handle_fn = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "ayiou_handle")
            .context("WASM plugin missing 'ayiou_handle' export")?;

        let result_ptr = handle_fn.call(&mut store, (ctx_ptr, ctx_json.len() as i32))?;

        // Read response
        let response_json = Self::read_guest_string(&store, &memory, result_ptr)?;
        let response: WasmHandleResponse =
            serde_json::from_str(&response_json).context("Failed to parse WASM plugin response")?;

        // Handle reply if present
        if let Some(reply) = response.reply {
            ctx.reply_text(reply).await?;
        }

        // Also check for pending reply set via host_reply
        let pending = {
            let mut state = self.host_state.lock();
            state.pending_reply.take()
        };
        if let Some(reply) = pending {
            ctx.reply_text(reply).await?;
        }

        Ok(response.block)
    }

    async fn on_load(&self) -> Result<()> {
        info!("WASM plugin loaded: {}", self.metadata.name);
        Ok(())
    }

    async fn on_unload(&self) -> Result<()> {
        info!("WASM plugin unloaded: {}", self.metadata.name);
        Ok(())
    }

    async fn on_before_reload(&self) -> Result<serde_json::Value> {
        let state = self.host_state.lock();
        Ok(serde_json::to_value(&state.state)?)
    }

    async fn on_after_reload(&self, saved_state: serde_json::Value) -> Result<()> {
        if let Ok(state_map) = serde_json::from_value(saved_state) {
            let mut state = self.host_state.lock();
            state.state = state_map;
        }
        Ok(())
    }
}

// Make WasmPlugin Send + Sync safe
unsafe impl Send for WasmPlugin {}
unsafe impl Sync for WasmPlugin {}
