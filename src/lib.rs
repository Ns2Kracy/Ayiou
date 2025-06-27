use std::sync::Arc;

use crate::app::config::ConfigManager;

pub mod app;
pub mod error;
pub mod middleware;
pub mod utils;

#[derive(Clone)]
pub struct Context {
    pub config: ConfigManager,
}

pub type Ctx = Arc<Context>;
