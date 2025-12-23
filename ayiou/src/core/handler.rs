use std::future::Future;

use anyhow::Result;
use futures::{FutureExt, future::BoxFuture};

use crate::core::{adapter::MsgContext, extract::FromEvent};

/// A trait for async functions that can handle events
pub trait Handler<C, Args>: Clone + Send + Sync + 'static {
    fn call(&self, ctx: C) -> BoxFuture<'static, Result<()>>;
}

/// Implement Handler for functions with 0 args
impl<Func, Fut, C> Handler<C, ()> for Func
where
    Func: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
{
    fn call(&self, _ctx: C) -> BoxFuture<'static, Result<()>> {
        let fut = (self)();
        fut.boxed()
    }
}

/// Implement Handler for functions with 1 arg
impl<Func, Fut, C, T1> Handler<C, (T1,)> for Func
where
    Func: Fn(T1) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
    T1: FromEvent<C> + Send + 'static,
{
    fn call(&self, ctx: C) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            func(t1).await
        }
        .boxed()
    }
}

/// Implement Handler for functions with 2 args
impl<Func, Fut, C, T1, T2> Handler<C, (T1, T2)> for Func
where
    Func: Fn(T1, T2) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
    T1: FromEvent<C> + Send + 'static,
    T2: FromEvent<C> + Send + 'static,
{
    fn call(&self, ctx: C) -> BoxFuture<'static, Result<()>> {
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
impl<Func, Fut, C, T1, T2, T3> Handler<C, (T1, T2, T3)> for Func
where
    Func: Fn(T1, T2, T3) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
    T1: FromEvent<C> + Send + 'static,
    T2: FromEvent<C> + Send + 'static,
    T3: FromEvent<C> + Send + 'static,
{
    fn call(&self, ctx: C) -> BoxFuture<'static, Result<()>> {
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
pub async fn call_handler<H, C, Args>(handler: H, ctx: C) -> Result<()>
where
    H: Handler<C, Args>,
{
    handler.call(ctx).await
}

/// Trait for handlers that take a specific Command Argument (parsed from text)
/// plus other extractors.
pub trait CommandHandlerFn<C, Args, CmdArgs>: Clone + Send + Sync + 'static {
    fn call(&self, ctx: C, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>>;
}

/// Implement for functions with (CmdArgs)
impl<Func, Fut, C, CmdArgs> CommandHandlerFn<C, (), CmdArgs> for Func
where
    Func: Fn(CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    CmdArgs: Send + 'static,
    C: MsgContext,
{
    fn call(&self, _ctx: C, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move { func(cmd_args).await }.boxed()
    }
}

/// Implement for functions with (T1, CmdArgs) - Common pattern: (UserId, Args)
impl<Func, Fut, C, T1, CmdArgs> CommandHandlerFn<C, (T1,), CmdArgs> for Func
where
    Func: Fn(T1, CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
    T1: FromEvent<C> + Send + 'static,
    CmdArgs: Send + 'static,
{
    fn call(&self, ctx: C, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
        let func = self.clone();
        async move {
            let t1 = T1::from_event(&ctx).await.map_err(Into::into)?;
            func(t1, cmd_args).await
        }
        .boxed()
    }
}

/// Implement for functions with (T1, T2, CmdArgs)
impl<Func, Fut, C, T1, T2, CmdArgs> CommandHandlerFn<C, (T1, T2), CmdArgs> for Func
where
    Func: Fn(T1, T2, CmdArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    C: MsgContext,
    T1: FromEvent<C> + Send + 'static,
    T2: FromEvent<C> + Send + 'static,
    CmdArgs: Send + 'static,
{
    fn call(&self, ctx: C, cmd_args: CmdArgs) -> BoxFuture<'static, Result<()>> {
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
pub async fn call_command_handler<H, C, Args, CmdArgs>(
    handler: H,
    ctx: C,
    cmd_args: CmdArgs,
) -> Result<()>
where
    H: CommandHandlerFn<C, Args, CmdArgs>,
{
    handler.call(ctx, cmd_args).await
}
