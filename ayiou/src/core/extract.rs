use crate::adapter::onebot::v11::ctx::Ctx;
use anyhow::Result;
use async_trait::async_trait;

/// Trait for types that can be extracted from a context
#[async_trait]
pub trait FromEvent: Sized {
    type Error: Into<anyhow::Error>;

    /// Extract data from the context
    async fn from_event(ctx: &Ctx) -> std::result::Result<Self, Self::Error>;
}

/// Implement for Ctx itself (identity)
#[async_trait]
impl FromEvent for Ctx {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        Ok(ctx.clone())
    }
}

/// Implement for String (Raw Message Text)
#[async_trait]
impl FromEvent for String {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        Ok(ctx.text())
    }
}

/// Optional extractor
#[async_trait]
impl<T: FromEvent + Send> FromEvent for Option<T> {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        match T::from_event(ctx).await {
            Ok(val) => Ok(Some(val)),
            Err(_) => Ok(None),
        }
    }
}

/// Extractor for UserId
pub struct UserId(pub i64);

#[async_trait]
impl FromEvent for UserId {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        Ok(UserId(ctx.user_id()))
    }
}

/// Extractor for GroupId (optional, since it might be private message)
pub struct GroupId(pub Option<i64>);

#[async_trait]
impl FromEvent for GroupId {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        Ok(GroupId(ctx.group_id()))
    }
}

use crate::core::plugin::Args;

/// Trait for types that can be constructed from Context + Command Arguments
#[async_trait]
pub trait FromContext: Sized {
    async fn from_ctx(ctx: &Ctx, args: &str) -> Result<Self, anyhow::Error>;
}

/// Blanket implementation for types that implement Args (backward compatibility)
#[async_trait]
impl<T: Args + Send> FromContext for T {
    async fn from_ctx(_ctx: &Ctx, args: &str) -> Result<Self, anyhow::Error> {
        T::parse(args).map_err(|e| e.into())
    }
}
