#![forbid(unsafe_code)]
#![allow(unused)]

use std::sync::Arc;

use axum::Json;
use mimalloc::MiMalloc;
use moka::future::Cache;
use sqlx::PgPool;

use crate::error::AyiouError;

pub mod app;
pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod utils;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub type ApiResult<T> = Result<Json<T>, AyiouError>;

#[derive(Clone)]
pub struct Context {
    pub db: PgPool,
    pub cache: Cache<String, String>,
}

pub type Ctx = Arc<Context>;
