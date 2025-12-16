use std::future::Future;

use anyhow::Result;
use futures::{FutureExt, future::BoxFuture};

use crate::{adapter::onebot::v11::ctx::Ctx, core::extract::FromEvent};

/// A trait for async functions that can handle events
pub trait Handler<Args>: Clone + Send + Sync + 'static {
    fn call(&self, ctx: Ctx) -> BoxFuture<'static, Result<()>>;
}

/// Implement Handler for functions with 0 args
impl<Func, Fut> Handler<()> for Func
where
    Func: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    fn call(&self, _ctx: Ctx) -> BoxFuture<'static, Result<()>> {
        let fut = (self)();
        fut.boxed()
    }
}

/// Implement Handler for functions with 1 arg
impl<Func, Fut, T1> Handler<(T1,)> for Func
where
    Func: Fn(T1) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    T1: FromEvent + Send + 'static,
{
    fn call(&self, ctx: Ctx) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            func(t1).await
        }
        .boxed()
    }
}

/// Implement Handler for functions with 2 args
impl<Func, Fut, T1, T2> Handler<(T1, T2)> for Func
where
    Func: Fn(T1, T2) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    T1: FromEvent + Send + 'static,
    T2: FromEvent + Send + 'static,
{
    fn call(&self, ctx: Ctx) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            let t2 = T2::from_event(&ctx).await.map_err(Into::into)?;
            func(t1, t2).await
        }
        .boxed()
    }
}

/// Implement Handler for functions with 3 args
impl<Func, Fut, T1, T2, T3> Handler<(T1, T2, T3)> for Func
where
    Func: Fn(T1, T2, T3) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    T1: FromEvent + Send + 'static,
    T2: FromEvent + Send + 'static,
    T3: FromEvent + Send + 'static,
{
    fn call(&self, ctx: Ctx) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            let t2 = T2::from_event(&ctx).await.map_err(Into::into)?;
            let t3 = T3::from_event(&ctx).await.map_err(Into::into)?;
            func(t1, t2, t3).await
        }
        .boxed()
    }
}

/// Helper to call a handler (used by macros)
pub async fn call_handler<H, Args>(handler: H, ctx: Ctx) -> Result<()>
where
    H: Handler<Args>,
{
    handler.call(ctx).await
}

/// Trait for handlers that take a specific Command Argument (parsed from text)
/// plus other extractors.
pub trait CommandHandlerFn<Args, CmdArgs>: Clone + Send + Sync + 'static {
    fn call(&self, ctx: Ctx, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>>;
}

/// Implement for functions with (CmdArgs)
impl<Func, Fut, CmdArgs> CommandHandlerFn<(), CmdArgs> for Func
where
    Func: Fn(CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    CmdArgs: Send + 'static,
{
    fn call(&self, _ctx: Ctx, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move { func(cmd_args).await }.boxed()
    }
}

/// Implement for functions with (T1, CmdArgs) - Common pattern: (UserId, Args)
impl<Func, Fut, T1, CmdArgs> CommandHandlerFn<(T1,), CmdArgs> for Func
where
    Func: Fn(T1, CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    T1: FromEvent + Send + 'static,
    CmdArgs: Send + 'static,
{
    fn call(&self, ctx: Ctx, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            func(t1, cmd_args).await
        }
        .boxed()
    }
}

/// Implement for functions with (T1, T2, CmdArgs)
impl<Func, Fut, T1, T2, CmdArgs> CommandHandlerFn<(T1, T2), CmdArgs> for Func
where
    Func: Fn(T1, T2, CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    T1: FromEvent + Send + 'static,
    T2: FromEvent + Send + 'static,
    CmdArgs: Send + 'static,
{
    fn call(&self, ctx: Ctx, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            let t2 = T2::from_event(&ctx).await.map_err(Into::into)?;
            func(t1, t2, cmd_args).await
        }
        .boxed()
    }
}

/// Helper to call a command handler (used by macros)
pub async fn call_command_handler<H, Args, CmdArgs>(
    handler: H,
    ctx: Ctx,
    cmd_args: CmdArgs,
) -> Result<()>
where
    H: CommandHandlerFn<Args, CmdArgs>,
{
    handler.call(ctx, cmd_args).await
}
