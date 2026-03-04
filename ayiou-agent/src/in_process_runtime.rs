use std::{collections::HashSet, sync::Arc};

use anyhow::{Context, Result, bail};
use ayiou::{NoopWasmHost, RuntimeController, WasmRuntime, core::PluginRuntimeState};
use ayiou_admin_proto::ConfigBackend;
use dashmap::DashMap;

use crate::executor::RuntimeOps;

#[derive(Clone, Default)]
pub struct InProcessRuntimeOps {
    bots: Arc<DashMap<String, Arc<BotState>>>,
}

impl InProcessRuntimeOps {
    fn bot_state(&self, bot_id: &str) -> Arc<BotState> {
        self.bots
            .entry(bot_id.to_string())
            .or_insert_with(|| Arc::new(BotState::default()))
            .clone()
    }

    pub async fn is_wasm_loaded(&self, bot_id: &str, module_name: &str) -> bool {
        let state = self.bot_state(bot_id);
        let modules = state.wasm.loaded_modules().await;
        modules.iter().any(|name| name == module_name)
    }
}

#[async_trait::async_trait]
impl RuntimeOps for InProcessRuntimeOps {
    async fn start_bot(&self, bot_id: &str) -> Result<()> {
        self.bot_state(bot_id).runtime.start().await
    }

    async fn stop_bot(&self, bot_id: &str) -> Result<()> {
        self.bot_state(bot_id).runtime.stop().await
    }

    async fn set_plugin_enabled(
        &self,
        bot_id: &str,
        plugin_name: &str,
        enabled: bool,
    ) -> Result<()> {
        self.bot_state(bot_id)
            .plugins
            .set_enabled(plugin_name, enabled);
        Ok(())
    }

    async fn update_plugin_config(
        &self,
        bot_id: &str,
        plugin_name: &str,
        _backend: ConfigBackend,
        _content: &str,
        expected_version: Option<u64>,
    ) -> Result<()> {
        let state = self.bot_state(bot_id);
        let actual = state
            .configs
            .get(plugin_name)
            .map(|entry| entry.version)
            .unwrap_or(0);

        if let Some(expected) = expected_version
            && expected != actual
        {
            bail!("version conflict: expected {}, actual {}", expected, actual);
        }

        let next = actual.checked_add(1).context("config version overflow")?;
        state
            .configs
            .insert(plugin_name.to_string(), StoredConfig { version: next });
        Ok(())
    }

    async fn load_wasm_plugin(
        &self,
        bot_id: &str,
        plugin_name: &str,
        module_path: &str,
    ) -> Result<()> {
        let state = self.bot_state(bot_id);
        let before: HashSet<String> = state.wasm.loaded_modules().await.into_iter().collect();
        state.wasm.load_module(module_path).await?;
        let after: HashSet<String> = state.wasm.loaded_modules().await.into_iter().collect();

        if after.contains(plugin_name) {
            state
                .wasm_alias
                .insert(plugin_name.to_string(), plugin_name.to_string());
            return Ok(());
        }

        let new_modules: Vec<String> = after.difference(&before).cloned().collect();
        if new_modules.len() == 1 {
            state
                .wasm_alias
                .insert(plugin_name.to_string(), new_modules[0].clone());
            return Ok(());
        }

        bail!(
            "loaded wasm module could not be resolved for plugin '{}'",
            plugin_name
        );
    }

    async fn unload_wasm_plugin(&self, bot_id: &str, plugin_name: &str) -> Result<()> {
        let state = self.bot_state(bot_id);
        let module_name = state
            .wasm_alias
            .get(plugin_name)
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| plugin_name.to_string());

        let removed = state.wasm.unload_module(&module_name).await?;
        if !removed {
            bail!("wasm module '{}' is not loaded", module_name);
        }

        state.wasm_alias.remove(plugin_name);
        Ok(())
    }
}

struct BotState {
    runtime: RuntimeController,
    plugins: PluginRuntimeState,
    configs: DashMap<String, StoredConfig>,
    wasm: WasmRuntime,
    wasm_alias: DashMap<String, String>,
}

impl Default for BotState {
    fn default() -> Self {
        Self {
            runtime: RuntimeController::default(),
            plugins: PluginRuntimeState::default(),
            configs: DashMap::new(),
            wasm: WasmRuntime::new(NoopWasmHost),
            wasm_alias: DashMap::new(),
        }
    }
}

#[derive(Clone)]
struct StoredConfig {
    version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!("{}/../ayiou/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    #[tokio::test]
    async fn runtime_ops_loads_and_unloads_real_wasm_module() {
        let runtime = InProcessRuntimeOps::default();
        runtime
            .load_wasm_plugin("bot-a", "echo", &fixture("echo_plugin_real.wasm"))
            .await
            .unwrap();
        assert!(runtime.is_wasm_loaded("bot-a", "echo").await);

        runtime.unload_wasm_plugin("bot-a", "echo").await.unwrap();
        assert!(!runtime.is_wasm_loaded("bot-a", "echo").await);
    }

    #[tokio::test]
    async fn config_update_requires_matching_expected_version() {
        let runtime = InProcessRuntimeOps::default();
        runtime
            .update_plugin_config("bot-a", "echo", ConfigBackend::Toml, "v1", None)
            .await
            .unwrap();

        let err = runtime
            .update_plugin_config("bot-a", "echo", ConfigBackend::Toml, "v2", Some(3))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("version conflict"));

        let state = runtime.bot_state("bot-a");
        let entry = state.configs.get("echo").unwrap();
        assert_eq!(entry.version, 1);
    }
}
