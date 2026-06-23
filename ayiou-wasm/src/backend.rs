use std::{path::PathBuf, sync::Arc};

use crate::{
    plugin::WasmRuntimePlugin,
    types::{WasmHandleOutcomeDto, WasmHandlerDto, WasmManifestDto},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ayiou::core::plugin::{PluginReloader, RegisteredPlugin};
use wasmtime::{Config, Engine, component::Component};

#[derive(Debug, Clone)]
pub struct WasmPluginSource {
    pub instance_id: String,
    pub artifact_path: PathBuf,
}

#[derive(Clone)]
pub struct WasmPluginBackend {
    engine: Arc<Engine>,
}

impl WasmPluginBackend {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    pub fn compile_component(&self, source: &WasmPluginSource) -> Result<Component> {
        Component::from_file(&self.engine, &source.artifact_path).with_context(|| {
            format!(
                "failed to compile wasm plugin `{}` from `{}`",
                source.instance_id,
                source.artifact_path.display()
            )
        })
    }

    pub async fn load_plugin(&self, source: WasmPluginSource) -> Result<WasmRuntimePlugin> {
        self.compile_component(&source)?;
        let manifest: WasmManifestDto = read_json_sidecar(&source, "manifest.json")?;
        let handlers: Vec<WasmHandlerDto> = read_json_sidecar(&source, "handlers.json")?;
        let handle_outcome = read_json_sidecar(&source, "handle-outcome.json")
            .unwrap_or(WasmHandleOutcomeDto { block: false });
        WasmRuntimePlugin::from_dtos(source.instance_id, manifest, handlers, handle_outcome)
    }

    pub fn engine(&self) -> Arc<Engine> {
        self.engine.clone()
    }
}

#[async_trait]
impl PluginReloader for WasmPluginBackend {
    async fn reload(&self, instance_id: &str) -> Result<RegisteredPlugin> {
        Err(anyhow::anyhow!(
            "wasm plugin `{instance_id}` reload requires a WasmPluginSource; use WasmPluginSourceReloader"
        ))
    }
}

#[derive(Clone)]
pub struct WasmPluginSourceReloader {
    backend: WasmPluginBackend,
    source: WasmPluginSource,
}

impl WasmPluginSourceReloader {
    #[must_use]
    pub const fn new(backend: WasmPluginBackend, source: WasmPluginSource) -> Self {
        Self { backend, source }
    }
}

#[async_trait]
impl PluginReloader for WasmPluginSourceReloader {
    async fn reload(&self, instance_id: &str) -> Result<RegisteredPlugin> {
        let plugin = self.backend.load_plugin(self.source.clone()).await?;
        Ok(
            RegisteredPlugin::new(instance_id.to_string(), Box::new(plugin))
                .with_reloader(Arc::new(self.clone())),
        )
    }
}

fn read_json_sidecar<T>(source: &WasmPluginSource, name: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let path = source.artifact_path.with_file_name(name);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read wasm plugin sidecar `{}`", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse wasm plugin sidecar `{}`", path.display()))
}
