use std::sync::Arc;

use dashmap::DashMap;

#[derive(Default, Clone)]
pub struct PluginRuntimeState {
    enabled: Arc<DashMap<String, bool>>,
}

impl PluginRuntimeState {
    pub fn set_enabled(&self, plugin: &str, on: bool) {
        self.enabled.insert(plugin.to_string(), on);
    }

    pub fn is_enabled(&self, plugin: &str) -> bool {
        self.enabled.get(plugin).map(|v| *v).unwrap_or(true)
    }
}
