use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

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

pub type PluginBox = Box<dyn Plugin>;

// ============================================================================
// PluginManager - 插件管理器（无锁设计）
// ============================================================================

type PluginList = Arc<[Arc<dyn Plugin>]>;

/// 插件管理器
///
/// 使用 Arc 快照模式，事件分发时无锁。
/// 构建阶段使用 Vec 收集插件，调用 `build()` 后生成不可变快照。
#[derive(Clone)]
pub struct PluginManager {
    /// 构建阶段的插件列表
    pending: Vec<Arc<dyn Plugin>>,
    /// 运行时的插件快照（不可变，无锁访问）
    snapshot: Option<PluginList>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            snapshot: None,
        }
    }

    /// 注册插件（构建阶段）
    pub fn register<P: Plugin>(&mut self, plugin: P) {
        let meta = plugin.meta();
        info!(
            "Registering plugin: {} v{} - {}",
            meta.name, meta.version, meta.description
        );
        self.pending.push(Arc::new(plugin));
    }

    /// 批量注册插件（支持不同类型的插件）
    pub fn register_all(&mut self, plugins: impl IntoIterator<Item = PluginBox>) {
        for plugin in plugins {
            let meta = plugin.meta();
            info!(
                "Registering plugin: {} v{} - {}",
                meta.name, meta.version, meta.description
            );
            self.pending.push(Arc::from(plugin));
        }
    }

    /// 构建快照（调用后插件列表不可变，用于高性能事件分发）
    pub fn build(&mut self) -> PluginList {
        let snapshot: PluginList = self.pending.drain(..).collect();
        self.snapshot = Some(snapshot.clone());
        snapshot
    }

    /// 获取快照（如果已构建）
    pub fn snapshot(&self) -> Option<PluginList> {
        self.snapshot.clone()
    }

    /// 获取所有插件元数据
    pub fn list(&self) -> Vec<PluginMetadata> {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().map(|p| p.meta()).collect()
        } else {
            self.pending.iter().map(|p| p.meta()).collect()
        }
    }

    /// 获取插件数量
    pub fn count(&self) -> usize {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.len()
        } else {
            self.pending.len()
        }
    }

    /// 检查插件是否存在
    pub fn has(&self, name: &str) -> bool {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().any(|p| p.meta().name == name)
        } else {
            self.pending.iter().any(|p| p.meta().name == name)
        }
    }
}

// ============================================================================
// Dispatcher - 事件分发器（无锁、高并发）
// ============================================================================

/// 事件分发器
///
/// 持有插件快照的引用，事件分发时完全无锁。
#[derive(Clone)]
pub struct Dispatcher {
    plugins: PluginList,
}

impl Dispatcher {
    /// 从插件列表创建分发器
    pub fn new(plugins: PluginList) -> Self {
        Self { plugins }
    }

    /// 分发事件到所有匹配的插件（顺序执行，支持阻断）
    pub async fn dispatch(&self, ctx: &Ctx) -> Result<()> {
        for plugin in self.plugins.iter() {
            if !plugin.matches(ctx) {
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

    /// 并发分发事件到所有匹配的插件（不支持阻断，全部并发执行）
    pub async fn dispatch_concurrent(&self, ctx: &Ctx) {
        let tasks: Vec<_> = self
            .plugins
            .iter()
            .filter(|p| p.matches(ctx))
            .map(|plugin| {
                let plugin = plugin.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let meta = plugin.meta();
                    if let Err(err) = plugin.handle(ctx).await {
                        tracing::error!("Plugin {} failed: {}", meta.name, err);
                    }
                })
            })
            .collect();

        // 等待所有任务完成
        for task in tasks {
            let _ = task.await;
        }
    }
}
