use std::{path::PathBuf, sync::Arc};

use crate::{plugin::WasmRuntimePlugin, types::WasmPluginPackageDto};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ayiou::core::plugin::{PluginReloader, RegisteredPlugin};
use wasmtime::{Config, Engine, component::Component};

#[derive(Debug, Clone)]
pub struct WasmPluginSource {
    pub instance_id: String,
    pub artifact_path: PathBuf,
    pub package_path: Option<PathBuf>,
}

impl WasmPluginSource {
    #[must_use]
    pub fn new(instance_id: impl Into<String>, artifact_path: impl Into<PathBuf>) -> Self {
        Self {
            instance_id: instance_id.into(),
            artifact_path: artifact_path.into(),
            package_path: None,
        }
    }

    #[must_use]
    pub fn package_path(mut self, package_path: impl Into<PathBuf>) -> Self {
        self.package_path = Some(package_path.into());
        self
    }
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
        let package = read_package(&source)?;
        WasmRuntimePlugin::from_package(source.instance_id, package)
    }

    pub async fn load_registered(&self, source: WasmPluginSource) -> Result<RegisteredPlugin> {
        let instance_id = source.instance_id.clone();
        let plugin = self.load_plugin(source.clone()).await?;
        Ok(
            RegisteredPlugin::new(instance_id, Box::new(plugin)).with_reloader(Arc::new(
                WasmPluginSourceReloader::new(self.clone(), source),
            )),
        )
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
        let mut source = self.source.clone();
        source.instance_id = instance_id.to_string();
        self.backend.load_registered(source).await
    }
}

fn read_package(source: &WasmPluginSource) -> Result<WasmPluginPackageDto> {
    let path = source
        .package_path
        .clone()
        .unwrap_or_else(|| source.artifact_path.with_file_name("ayiou-plugin.json"));
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read wasm plugin package `{}`", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse wasm plugin package `{}`", path.display()))
}
