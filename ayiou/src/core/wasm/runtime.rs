use std::{
    collections::HashMap,
    path::Path,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use ayiou_wasm_sdk::HostCall;
use regex::Regex;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::core::wasm::host_api::{NoopWasmHost, WasmHostApi};

#[derive(Clone)]
pub struct WasmRuntime {
    host: Arc<dyn WasmHostApi>,
    modules: Arc<RwLock<HashMap<String, LoadedModule>>>,
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new(NoopWasmHost)
    }
}

impl WasmRuntime {
    pub fn new(host: impl WasmHostApi) -> Self {
        Self {
            host: Arc::new(host),
            modules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn load_module(&self, path: &str) -> Result<()> {
        let source = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("read wasm module manifest from {}", path))?;
        let manifest: WasmModuleManifest = toml::from_str(&source)
            .with_context(|| format!("parse wasm module manifest from {}", path))?;

        let module_name = manifest.name.clone().unwrap_or_else(|| fallback_name(path));
        let trigger = ModuleTrigger::from_manifest(manifest)
            .with_context(|| format!("build module trigger for {}", module_name))?;

        if trigger.command.is_none() && trigger.regex.is_none() && trigger.cron.is_none() {
            bail!(
                "module '{}' has no dispatch trigger (command/regex/cron)",
                module_name
            );
        }

        self.modules.write().await.insert(
            module_name.clone(),
            LoadedModule {
                name: module_name,
                trigger,
            },
        );
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
        let calls = {
            let modules = self.modules.read().await;
            modules
                .values()
                .filter(|module| module.trigger.command.as_deref() == Some(command))
                .map(|module| HostCall::command(module.name.clone(), command, args))
                .collect::<Vec<_>>()
        };

        for call in &calls {
            self.host.on_call(call.clone()).await?;
        }

        Ok(!calls.is_empty())
    }

    pub async fn dispatch_regex(&self, text: &str) -> Result<bool> {
        let calls = {
            let modules = self.modules.read().await;
            modules
                .values()
                .filter(|module| {
                    module
                        .trigger
                        .regex
                        .as_ref()
                        .is_some_and(|pattern| pattern.is_match(text))
                })
                .map(|module| HostCall::regex(module.name.clone(), text))
                .collect::<Vec<_>>()
        };

        for call in &calls {
            self.host.on_call(call.clone()).await?;
        }

        Ok(!calls.is_empty())
    }

    pub async fn trigger_cron(&self, expr: &str) -> Result<bool> {
        let _ = cron::Schedule::from_str(expr).context("invalid runtime cron expression")?;

        let calls = {
            let modules = self.modules.read().await;
            modules
                .values()
                .filter(|module| module.trigger.cron.as_deref() == Some(expr))
                .map(|module| HostCall::cron(module.name.clone(), expr))
                .collect::<Vec<_>>()
        };

        for call in &calls {
            self.host.on_call(call.clone()).await?;
        }

        Ok(!calls.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct WasmModuleManifest {
    name: Option<String>,
    command: Option<String>,
    regex: Option<String>,
    cron: Option<String>,
}

#[derive(Clone)]
struct LoadedModule {
    name: String,
    trigger: ModuleTrigger,
}

#[derive(Clone)]
struct ModuleTrigger {
    command: Option<String>,
    regex: Option<Regex>,
    cron: Option<String>,
}

impl ModuleTrigger {
    fn from_manifest(manifest: WasmModuleManifest) -> Result<Self> {
        let regex = match manifest.regex {
            Some(pattern) => Some(
                Regex::new(&pattern)
                    .with_context(|| format!("invalid regex trigger '{}'", pattern))?,
            ),
            None => None,
        };

        if let Some(expr) = manifest.cron.as_deref() {
            let _ = cron::Schedule::from_str(expr)
                .with_context(|| format!("invalid cron trigger '{}'", expr))?;
        }

        Ok(Self {
            command: manifest.command,
            regex,
            cron: manifest.cron,
        })
    }
}

fn fallback_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "unnamed".to_string())
}

#[cfg(test)]
mod tests {
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
}
