use crate::onebot::api::Api;

/// 为插件提供的上下文
///
/// 每个事件都会构建一个新的 Ctx，包含了对应 Bot 实例的 API 客户端。
#[derive(Clone)]
pub struct Ctx {
    /// 用于调用 OneBot API 的客户端
    pub api: Api,
}

impl Ctx {
    pub fn new(api: Api) -> Self {
        Self { api }
    }
}
