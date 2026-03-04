use std::{collections::HashMap, path::Path, str::FromStr, sync::Arc};

use anyhow::{Context, Result, bail};
use ayiou_wasm_sdk::{HostCall, abi};
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
            .with_context(|| format!("compile wasm module {}", path))?;
        let mut store = Store::new(&self.engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .with_context(|| format!("instantiate wasm module {}", path))?;

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
        let modules = snapshot_modules(&self.modules).await;
        let mut any = false;

        for module in modules {
            if module.call_command(command, args).await? {
                any = true;
                self.host
                    .on_call(HostCall::command(module.name.clone(), command, args))
                    .await?;
            }
        }

        Ok(any)
    }

    pub async fn dispatch_regex(&self, text: &str) -> Result<bool> {
        let modules = snapshot_modules(&self.modules).await;
        let mut any = false;

        for module in modules {
            if module.call_regex(text).await? {
                any = true;
                self.host
                    .on_call(HostCall::regex(module.name.clone(), text))
                    .await?;
            }
        }

        Ok(any)
    }

    pub async fn trigger_cron(&self, expr: &str) -> Result<bool> {
        let _ = cron::Schedule::from_str(expr).context("invalid runtime cron expression")?;

        let modules = snapshot_modules(&self.modules).await;
        let mut any = false;

        for module in modules {
            if module.call_cron(expr).await? {
                any = true;
                self.host
                    .on_call(HostCall::cron(module.name.clone(), expr))
                    .await?;
            }
        }

        Ok(any)
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

impl LoadedModule {
    async fn call_command(&self, command: &str, args: &str) -> Result<bool> {
        let mut guard = self.inner.lock().await;
        guard.call_command(command, args)
    }

    async fn call_regex(&self, text: &str) -> Result<bool> {
        let mut guard = self.inner.lock().await;
        guard.call_regex(text)
    }

    async fn call_cron(&self, expr: &str) -> Result<bool> {
        let mut guard = self.inner.lock().await;
        guard.call_cron(expr)
    }
}

impl LoadedInstance {
    fn call_command(&mut self, command: &str, args: &str) -> Result<bool> {
        let Some(func) = self
            .instance
            .get_func(&mut self.store, abi::ON_COMMAND_EXPORT)
        else {
            return Ok(false);
        };

        let func = func
            .typed::<(i32, i32, i32, i32), i32>(&self.store)
            .context("type-check command export")?;
        let (cmd_ptr, cmd_len) = self.write_utf8(command)?;
        let (args_ptr, args_len) = self.write_utf8(args)?;
        let result = func
            .call(&mut self.store, (cmd_ptr, cmd_len, args_ptr, args_len))
            .context("call command export")?;
        Ok(result != 0)
    }

    fn call_regex(&mut self, text: &str) -> Result<bool> {
        let Some(func) = self
            .instance
            .get_func(&mut self.store, abi::ON_REGEX_EXPORT)
        else {
            return Ok(false);
        };

        let func = func
            .typed::<(i32, i32), i32>(&self.store)
            .context("type-check regex export")?;
        let (ptr, len) = self.write_utf8(text)?;
        let result = func
            .call(&mut self.store, (ptr, len))
            .context("call regex export")?;
        Ok(result != 0)
    }

    fn call_cron(&mut self, expr: &str) -> Result<bool> {
        let Some(func) = self.instance.get_func(&mut self.store, abi::ON_CRON_EXPORT) else {
            return Ok(false);
        };

        let func = func
            .typed::<(i32, i32), i32>(&self.store)
            .context("type-check cron export")?;
        let (ptr, len) = self.write_utf8(expr)?;
        let result = func
            .call(&mut self.store, (ptr, len))
            .context("call cron export")?;
        Ok(result != 0)
    }

    fn write_utf8(&mut self, text: &str) -> Result<(i32, i32)> {
        let bytes = text.as_bytes();
        let len = i32::try_from(bytes.len()).context("wasm input too large")?;

        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, abi::ALLOC_EXPORT)
            .context("get alloc export")?;
        let ptr = alloc
            .call(&mut self.store, len)
            .context("call alloc export")?;
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
