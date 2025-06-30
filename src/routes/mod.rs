use axum::Router;

use crate::Ctx;

pub mod api;

pub fn mount() -> Router<Ctx> {
    Router::new().nest(
        "/api",
        Router::new()
            .merge(auth_routes())
            .merge(link_routes())
            .merge(user_routes())
            .merge(analytics_routes())
            .merge(redirect_routes()),
    )
}

fn auth_routes() -> Router<Ctx> {
    Router::new().nest("/auth", Router::new())
}

fn link_routes() -> Router<Ctx> {
    Router::new().nest("/links", Router::new())
}

fn user_routes() -> Router<Ctx> {
    Router::new().nest("/users", Router::new())
}

fn analytics_routes() -> Router<Ctx> {
    Router::new().nest("/analytics", Router::new())
}

fn redirect_routes() -> Router<Ctx> {
    Router::new()
}
