#![forbid(unsafe_code)]
#![allow(unused)]

use std::sync::Arc;

use axum::Json;
use mimalloc::MiMalloc;
use moka::future::Cache;
use sqlx::PgPool;

use crate::error::AyiouError;
use crate::services::{auth::AuthService, user::UserService};

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
    pub auth_service: Arc<AuthService>,
    pub user_service: Arc<UserService>,
}

impl Context {
    pub fn new(db: PgPool) -> Self {
        let cache = Cache::new(10_000);
        let auth_service = Arc::new(AuthService::new(db.clone()));
        let user_service = Arc::new(UserService::new(db.clone()));

        Self {
            db,
            cache,
            auth_service,
            user_service,
        }
    }
}

pub type Ctx = Arc<Context>;
