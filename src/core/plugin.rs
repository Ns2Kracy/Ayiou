use anyhow::Result;
use async_trait::async_trait;

use crate::core::ctx::Ctx;

// ============================================================================
// 元数据
// ============================================================================

#[derive(Clone, Debug)]
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
}

impl PluginMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }
}

impl Default for PluginMetadata {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }
}

/// 处理器 Trait：唯一入口
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// 元数据（名称/描述/版本）
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::default()
    }

    /// 是否匹配当前上下文（默认全匹配）
    fn matches(&self, _ctx: &Ctx) -> bool {
        true
    }

    /// 处理逻辑，返回 Ok(true) 表示阻止后续处理，Ok(false) 继续
    async fn handle(&self, ctx: Ctx) -> Result<bool>;
}

// ============================================================================
// PluginManager - 插件管理器
// ============================================================================

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// 插件管理器
pub struct PluginManager {
    plugins: Arc<RwLock<Vec<Box<dyn Plugin>>>>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册插件
    pub async fn register<P: Plugin>(&self, plugin: P) {
        let meta = plugin.meta();
        info!(
            "Registering plugin: {} v{} - {}",
            meta.name, meta.version, meta.description
        );
        self.plugins.write().await.push(Box::new(plugin));
    }

    /// 注销插件（按名称）
    pub async fn unregister(&self, name: &str) -> bool {
        let mut plugins = self.plugins.write().await;
        let len_before = plugins.len();
        plugins.retain(|p| p.meta().name != name);
        let removed = plugins.len() < len_before;
        if removed {
            info!("Unregistered plugin: {}", name);
        }
        removed
    }

    /// 获取所有插件元数据
    pub async fn list(&self) -> Vec<PluginMetadata> {
        self.plugins.read().await.iter().map(|p| p.meta()).collect()
    }

    /// 获取插件数量
    pub async fn count(&self) -> usize {
        self.plugins.read().await.len()
    }

    /// 检查插件是否存在
    pub async fn has(&self, name: &str) -> bool {
        self.plugins
            .read()
            .await
            .iter()
            .any(|p| p.meta().name == name)
    }

    /// 获取内部插件列表的引用（用于事件分发）
    pub fn plugins(&self) -> Arc<RwLock<Vec<Box<dyn Plugin>>>> {
        self.plugins.clone()
    }

    /// 分发事件到所有匹配的插件
    pub async fn dispatch(&self, ctx: Ctx) -> Result<()> {
        let plugins = self.plugins.read().await;

        for plugin in plugins.iter() {
            if !plugin.matches(&ctx) {
                continue;
            }

            let meta = plugin.meta();
            match plugin.handle(ctx.clone()).await {
                Ok(block) => {
                    if block {
                        break; // 阻止后续处理
                    }
                }
                Err(err) => {
                    tracing::error!("Plugin {} failed: {}", meta.name, err);
                }
            }
        }

        Ok(())
    }
}

impl Clone for PluginManager {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
        }
    }
}
