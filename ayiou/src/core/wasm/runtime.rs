use std::{collections::HashMap, path::Path, str::FromStr, sync::Arc};

use anyhow::{Context, Result, anyhow, bail};
use ayiou_wasm_sdk::{HostCall, ReplyAction, abi};
use tokio::sync::{Mutex, RwLock};
use wasmtime::{Engine, Instance, Module, Store};

use crate::core::wasm::host_api::{NoopWasmHost, WasmHostApi};

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Arc<Engine>,
    host: Arc<dyn WasmHostApi>,
    modules: Arc<RwLock<HashMap<String, Arc<LoadedModule>>>>,
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new(NoopWasmHost)
    }
}

impl WasmRuntime {
    pub fn new(host: impl WasmHostApi) -> Self {
        Self {
            engine: Arc::new(Engine::default()),
            host: Arc::new(host),
            modules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn load_module(&self, path: &str) -> Result<()> {
        let raw = tokio::fs::read(path)
            .await
            .with_context(|| format!("read wasm module from {}", path))?;
        let wasm =
            normalize_wasm_bytes(&raw).with_context(|| format!("decode wasm module {}", path))?;

        let module = Module::new(&self.engine, wasm)
            .map_err(|err| anyhow!("compile wasm module {}: {}", path, err))?;
        let mut store = Store::new(&self.engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|err| anyhow!("instantiate wasm module {}: {}", path, err))?;

        validate_abi(&mut store, &instance)?;

        let module_name = fallback_name(path);
        let loaded = Arc::new(LoadedModule {
            name: module_name.clone(),
            inner: Mutex::new(LoadedInstance { store, instance }),
        });

        self.modules.write().await.insert(module_name, loaded);
        Ok(())
    }

    pub async fn unload_module(&self, module_name: &str) -> Result<bool> {
        Ok(self.modules.write().await.remove(module_name).is_some())
    }

    pub async fn loaded_modules(&self) -> Vec<String> {
        let mut names: Vec<String> = self.modules.read().await.keys().cloned().collect();
        names.sort();
        names
    }

    pub async fn dispatch_command(&self, command: &str, args: &str) -> Result<bool> {
        Ok(!self.dispatch_command_calls(command, args).await?.is_empty())
    }

    pub async fn dispatch_command_calls(&self, command: &str, args: &str) -> Result<Vec<HostCall>> {
        let modules = snapshot_modules(&self.modules).await;
        let mut calls = Vec::new();

        for module in modules {
            let outcome = module.call_command(command, args).await?;
            if outcome.handled {
                let mut call = HostCall::command(module.name.clone(), command, args);
                if let Some(reply) = outcome.reply {
                    call = call.with_reply(reply);
                }

                self.host.on_call(call.clone()).await?;
                calls.push(call);
            }
        }

        Ok(calls)
    }

    pub async fn dispatch_regex(&self, text: &str) -> Result<bool> {
        Ok(!self.dispatch_regex_calls(text).await?.is_empty())
    }

    pub async fn dispatch_regex_calls(&self, text: &str) -> Result<Vec<HostCall>> {
        let modules = snapshot_modules(&self.modules).await;
        let mut calls = Vec::new();

        for module in modules {
            let outcome = module.call_regex(text).await?;
            if outcome.handled {
                let mut call = HostCall::regex(module.name.clone(), text);
                if let Some(reply) = outcome.reply {
                    call = call.with_reply(reply);
                }

                self.host.on_call(call.clone()).await?;
                calls.push(call);
            }
        }

        Ok(calls)
    }

    pub async fn trigger_cron(&self, expr: &str) -> Result<bool> {
        Ok(!self.trigger_cron_calls(expr).await?.is_empty())
    }

    pub async fn trigger_cron_calls(&self, expr: &str) -> Result<Vec<HostCall>> {
        let _ = cron::Schedule::from_str(expr).context("invalid runtime cron expression")?;

        let modules = snapshot_modules(&self.modules).await;
        let mut calls = Vec::new();

        for module in modules {
            let outcome = module.call_cron(expr).await?;
            if outcome.handled {
                let mut call = HostCall::cron(module.name.clone(), expr);
                if let Some(reply) = outcome.reply {
                    call = call.with_reply(reply);
                }

                self.host.on_call(call.clone()).await?;
                calls.push(call);
            }
        }

        Ok(calls)
    }
}

struct LoadedModule {
    name: String,
    inner: Mutex<LoadedInstance>,
}

struct LoadedInstance {
    store: Store<()>,
    instance: Instance,
}

struct DispatchOutcome {
    handled: bool,
    reply: Option<ReplyAction>,
}

impl DispatchOutcome {
    fn not_handled() -> Self {
        Self {
            handled: false,
            reply: None,
        }
    }
}

impl LoadedModule {
    async fn call_command(&self, command: &str, args: &str) -> Result<DispatchOutcome> {
        let mut guard = self.inner.lock().await;
        guard.call_command(command, args)
    }

    async fn call_regex(&self, text: &str) -> Result<DispatchOutcome> {
        let mut guard = self.inner.lock().await;
        guard.call_regex(text)
    }

    async fn call_cron(&self, expr: &str) -> Result<DispatchOutcome> {
        let mut guard = self.inner.lock().await;
        guard.call_cron(expr)
    }
}

impl LoadedInstance {
    fn call_command(&mut self, command: &str, args: &str) -> Result<DispatchOutcome> {
        let Some(func) = self
            .instance
            .get_func(&mut self.store, abi::ON_COMMAND_EXPORT)
        else {
            return Ok(DispatchOutcome::not_handled());
        };

        let func = func
            .typed::<(i32, i32, i32, i32), i32>(&self.store)
            .map_err(|err| anyhow!("type-check command export: {}", err))?;
        let (cmd_ptr, cmd_len) = self.write_utf8(command)?;
        let (args_ptr, args_len) = self.write_utf8(args)?;
        let result = func
            .call(&mut self.store, (cmd_ptr, cmd_len, args_ptr, args_len))
            .map_err(|err| anyhow!("call command export: {}", err))?;
        if result == 0 {
            return Ok(DispatchOutcome::not_handled());
        }

        Ok(DispatchOutcome {
            handled: true,
            reply: self.take_reply()?,
        })
    }

    fn call_regex(&mut self, text: &str) -> Result<DispatchOutcome> {
        let Some(func) = self
            .instance
            .get_func(&mut self.store, abi::ON_REGEX_EXPORT)
        else {
            return Ok(DispatchOutcome::not_handled());
        };

        let func = func
            .typed::<(i32, i32), i32>(&self.store)
            .map_err(|err| anyhow!("type-check regex export: {}", err))?;
        let (ptr, len) = self.write_utf8(text)?;
        let result = func
            .call(&mut self.store, (ptr, len))
            .map_err(|err| anyhow!("call regex export: {}", err))?;
        if result == 0 {
            return Ok(DispatchOutcome::not_handled());
        }

        Ok(DispatchOutcome {
            handled: true,
            reply: self.take_reply()?,
        })
    }

    fn call_cron(&mut self, expr: &str) -> Result<DispatchOutcome> {
        let Some(func) = self.instance.get_func(&mut self.store, abi::ON_CRON_EXPORT) else {
            return Ok(DispatchOutcome::not_handled());
        };

        let func = func
            .typed::<(i32, i32), i32>(&self.store)
            .map_err(|err| anyhow!("type-check cron export: {}", err))?;
        let (ptr, len) = self.write_utf8(expr)?;
        let result = func
            .call(&mut self.store, (ptr, len))
            .map_err(|err| anyhow!("call cron export: {}", err))?;
        if result == 0 {
            return Ok(DispatchOutcome::not_handled());
        }

        Ok(DispatchOutcome {
            handled: true,
            reply: self.take_reply()?,
        })
    }

    fn write_utf8(&mut self, text: &str) -> Result<(i32, i32)> {
        let bytes = text.as_bytes();
        let len = i32::try_from(bytes.len()).context("wasm input too large")?;

        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, abi::ALLOC_EXPORT)
            .map_err(|err| anyhow!("get alloc export: {}", err))?;
        let ptr = alloc
            .call(&mut self.store, len)
            .map_err(|err| anyhow!("call alloc export: {}", err))?;
        if ptr < 0 {
            bail!("invalid allocated pointer {}", ptr);
        }

        let memory = self
            .instance
            .get_memory(&mut self.store, abi::MEMORY_EXPORT)
            .context("get memory export")?;
        memory
            .write(&mut self.store, ptr as usize, bytes)
            .context("write module memory")?;

        Ok((ptr, len))
    }

    fn take_reply(&mut self) -> Result<Option<ReplyAction>> {
        let Some(func) = self
            .instance
            .get_func(&mut self.store, abi::TAKE_REPLY_EXPORT)
        else {
            return Ok(None);
        };

        let func = func
            .typed::<(), i64>(&self.store)
            .map_err(|err| anyhow!("type-check take_reply export: {}", err))?;
        let packed = func
            .call(&mut self.store, ())
            .map_err(|err| anyhow!("call take_reply export: {}", err))?;
        if packed == 0 {
            return Ok(None);
        }

        let (ptr, len) = unpack_ptr_len(packed)?;
        let ptr = usize::try_from(ptr).context("reply pointer must be positive")?;
        let len = usize::try_from(len).context("reply length must be positive")?;

        let memory = self
            .instance
            .get_memory(&mut self.store, abi::MEMORY_EXPORT)
            .context("get memory export")?;
        let mut payload = vec![0_u8; len];
        memory
            .read(&self.store, ptr, &mut payload)
            .context("read reply payload from module memory")?;

        let reply =
            serde_json::from_slice(&payload).context("decode wasm reply payload as JSON")?;
        Ok(Some(reply))
    }
}

async fn snapshot_modules(
    modules: &RwLock<HashMap<String, Arc<LoadedModule>>>,
) -> Vec<Arc<LoadedModule>> {
    modules.read().await.values().cloned().collect()
}

fn validate_abi(store: &mut Store<()>, instance: &Instance) -> Result<()> {
    instance
        .get_memory(&mut *store, abi::MEMORY_EXPORT)
        .context("missing memory export")?;
    instance
        .get_func(&mut *store, abi::ALLOC_EXPORT)
        .context("missing alloc export")?;

    let has_dispatch = instance
        .get_func(&mut *store, abi::ON_COMMAND_EXPORT)
        .is_some()
        || instance
            .get_func(&mut *store, abi::ON_REGEX_EXPORT)
            .is_some()
        || instance
            .get_func(&mut *store, abi::ON_CRON_EXPORT)
            .is_some();
    if !has_dispatch {
        bail!("module has no dispatch export");
    }

    Ok(())
}

fn normalize_wasm_bytes(raw: &[u8]) -> Result<Vec<u8>> {
    // If not a binary wasm payload, allow WAT text fixtures.
    if raw.starts_with(b"\0asm") {
        return Ok(raw.to_vec());
    }

    let bytes = wat::parse_bytes(raw).context("parse WAT source")?;
    Ok(bytes.into_owned())
}

fn unpack_ptr_len(packed: i64) -> Result<(i32, i32)> {
    let packed = packed as u64;
    let ptr = (packed & 0xFFFF_FFFF) as u32;
    let len = (packed >> 32) as u32;

    let ptr = i32::try_from(ptr).context("reply pointer overflow i32")?;
    let len = i32::try_from(len).context("reply length overflow i32")?;
    if ptr < 0 {
        bail!("invalid reply pointer {}", ptr);
    }
    if len < 0 {
        bail!("invalid reply length {}", len);
    }

    Ok((ptr, len))
}

fn fallback_name(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unnamed");

    let name = stem
        .strip_suffix("_plugin_real")
        .or_else(|| stem.strip_suffix("_plugin"))
        .unwrap_or(stem);

    name.to_string()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ayiou_wasm_sdk::DispatchEvent;

    use super::*;
    use crate::core::wasm::host_api::RecordingWasmHost;

    fn fixture(name: &str) -> String {
        format!("{}/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    fn test_host() -> RecordingWasmHost {
        RecordingWasmHost::default()
    }

    #[tokio::test]
    async fn wasm_plugin_handles_command_regex_and_cron() {
        let host = test_host();
        let rt = WasmRuntime::new(host.clone());
        rt.load_module(&fixture("echo_plugin.wasm")).await.unwrap();

        assert!(rt.dispatch_command("echo", "hi").await.unwrap());
        assert!(rt.dispatch_regex("https://example.com").await.unwrap());
        assert!(rt.trigger_cron("*/1 * * * * *").await.unwrap());

        let calls = host.calls().await;
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].plugin, "echo");
        assert_eq!(
            calls[0].event,
            DispatchEvent::Command {
                command: "echo".to_string(),
                args: "hi".to_string()
            }
        );
    }

    #[tokio::test]
    async fn wasm_runtime_supports_module_unload() {
        let rt = WasmRuntime::default();
        rt.load_module(&fixture("echo_plugin.wasm")).await.unwrap();
        assert!(rt.unload_module("echo").await.unwrap());
        assert!(!rt.dispatch_command("echo", "hi").await.unwrap());
    }

    #[tokio::test]
    async fn wasm_runtime_executes_real_wasm_abi_exports() {
        let host = test_host();
        let rt = WasmRuntime::new(host.clone());
        rt.load_module(&fixture("echo_plugin_real.wasm"))
            .await
            .unwrap();

        assert!(rt.dispatch_command("echo", "hi").await.unwrap());
        assert!(!rt.dispatch_command("ping", "hi").await.unwrap());
        assert!(rt.dispatch_regex("https://example.com").await.unwrap());
        assert!(rt.trigger_cron("*/1 * * * * *").await.unwrap());
    }

    #[test]
    fn fallback_name_trims_plugin_suffix() {
        assert_eq!(fallback_name("/tmp/echo_plugin.wasm"), "echo");
        assert_eq!(fallback_name("/tmp/echo_plugin_real.wasm"), "echo");
        assert_eq!(fallback_name("/tmp/raw.wasm"), "raw");
    }

    #[test]
    fn normalize_wasm_bytes_accepts_wat_text() {
        let bytes = normalize_wasm_bytes(b"(module)").unwrap();
        assert!(bytes.starts_with(b"\0asm"));
    }

    #[test]
    fn cron_expression_is_validated_before_dispatch() {
        let err = cron::Schedule::from_str("invalid cron").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid"));
    }
}
