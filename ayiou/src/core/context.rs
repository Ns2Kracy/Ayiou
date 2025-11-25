use std::{collections::HashMap, sync::Arc};

use crate::core::Adapter;

/// 上下文，持有 Adapter 引用，供 Plugin 发送消息
#[derive(Default, Clone)]
pub struct Ctx {
    adapters: HashMap<String, Arc<dyn Adapter>>,
}

impl Ctx {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册 Adapter
    pub fn register_adapter(&mut self, adapter: Arc<dyn Adapter>) {
        self.adapters.insert(adapter.name().to_string(), adapter);
    }

    /// 根据平台名获取 Adapter
    pub fn adapter(&self, platform: &str) -> Option<&Arc<dyn Adapter>> {
        self.adapters.get(platform)
    }
}
