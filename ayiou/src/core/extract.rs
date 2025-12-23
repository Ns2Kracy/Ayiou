use anyhow::Result;
use async_trait::async_trait;

use crate::core::adapter::MsgContext;

/// Trait for types that can be extracted from a context
#[async_trait]
pub trait FromEvent<C>: Sized {
    type Error: Into<anyhow::Error>;

    /// Extract data from the context
    async fn from_event(ctx: &C) -> std::result::Result<Self, Self::Error>;
}

/// Implement for Ctx itself (identity)
/// Constrained to MsgContext to avoid conflict with Option<T> impl
#[async_trait]
impl<C: MsgContext> FromEvent<C> for C {
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        Ok(ctx.clone())
    }
}

/// Implement for String (Raw Message Text)
#[async_trait]
impl<C: MsgContext> FromEvent<C> for String {
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        Ok(ctx.text())
    }
}

/// Optional extractor
#[async_trait]
impl<C, T> FromEvent<C> for Option<T>
where
    C: Send + Sync,
    T: FromEvent<C> + Send,
{
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        match T::from_event(ctx).await {
            Ok(val) => Ok(Some(val)),
            Err(_) => Ok(None),
        }
    }
}

/// Extractor for UserId (Generic as String)
pub struct UserId(pub String);

#[async_trait]
impl<C: MsgContext> FromEvent<C> for UserId {
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        Ok(UserId(ctx.user_id()))
    }
}

/// Extractor for GroupId (optional, since it might be private message)
pub struct GroupId(pub Option<String>);

#[async_trait]
impl<C: MsgContext> FromEvent<C> for GroupId {
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        Ok(GroupId(ctx.group_id()))
    }
}

use crate::core::plugin::Args;

/// Trait for types that can be constructed from Context + Command Arguments
#[async_trait]
pub trait FromContext<C>: Sized {
    async fn from_ctx(ctx: &C, args: &str) -> Result<Self, anyhow::Error>;
}

/// Blanket implementation for types that implement Args (backward compatibility)
#[async_trait]
impl<C: Send + Sync, T: Args + Send> FromContext<C> for T {
    async fn from_ctx(_ctx: &C, args: &str) -> Result<Self, anyhow::Error> {
        T::parse(args).map_err(|e| e.into())
    }
}
