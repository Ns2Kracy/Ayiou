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

use crate::core::plugin::ArgsParser;

/// Trait for types that can be constructed from Context + Command Arguments
#[async_trait]
pub trait FromContext<C>: Sized {
    async fn from_ctx(ctx: &C, args: &str) -> Result<Self, anyhow::Error>;
}

/// Blanket implementation for types that implement ArgsParser (backward compatibility)
#[async_trait]
impl<C: Send + Sync, T: ArgsParser + Send> FromContext<C> for T {
    async fn from_ctx(_ctx: &C, args: &str) -> Result<Self, anyhow::Error> {
        T::parse(args).map_err(|e| e.into())
    }
}

/// Wrapper to assert that T should be parsed from command arguments
pub struct Args<T>(pub T);

/// Trait for types that can be parsed from a list of arguments
pub trait TupleArgs: Sized {
    type Error: Into<anyhow::Error>;
    fn parse_from_iter<'a, I>(iter: &mut I) -> Result<Self, Self::Error>
    where
        I: Iterator<Item = &'a str>;
}

/// Implement Args<T> extractor
#[async_trait]
impl<C, T> FromEvent<C> for Args<T>
where
    C: MsgContext,
    T: TupleArgs + Send,
{
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        let text = ctx.text();
        // Simple heuristic: skip the first word (command) and parse the rest
        // NOTE: This assumes standart command syntax.
        // For more complex parsing, users should use custom extractors or the macro system.
        let args_str = text.trim();
        let mut parts = args_str.split_whitespace();

        // Skip valid command if it looks like one (e.g. starts with prefix or first word)
        // Since we don't know the exact command structure here without the Dispatcher context,
        // we'll optimistically consume the first word if it looks like a command invocation.
        // BUT, FromEvent doesn't know about specific plugin logic.
        // A safer bet for `Args` is to parse "everything after the command".
        // But we don't know what the command is.

        // Strategy:
        // If FromEvent is used, we assume the handler expects arguments.
        // We will assume the First word is the command and skip it.
        // This is consistent with how many bots works.
        // (If the handler is used for "wildcard" where there is no command, this might be wrong,
        // but wildcards usually consume the whole Text, not Args).
        parts.next();

        T::parse_from_iter(&mut parts)
            .map(Args)
            .map_err(|e| e.into())
    }
}

// Macro to implement TupleArgs for a single tuple size
// Called by all_the_tuples! with pattern: ([previous types], last_type)
macro_rules! impl_tuple_args {
    // Case: single element (first call: [], T1)
    ([], $T:ident) => {
        impl<$T> TupleArgs for ($T,)
        where
            $T: std::str::FromStr,
            $T::Err: Into<anyhow::Error>,
        {
            type Error = anyhow::Error;
            fn parse_from_iter<'a, I>(iter: &mut I) -> Result<Self, Self::Error>
            where
                I: Iterator<Item = &'a str>,
            {
                let s = iter.next().ok_or_else(|| anyhow::anyhow!("Missing argument 1"))?;
                let t = s.parse().map_err(|e: $T::Err| e.into())?;
                Ok((t,))
            }
        }
    };
    // Case: multiple elements ([T1, T2, ...], Tn)
    ([$($T:ident),+], $Last:ident) => {
        impl<$($T,)+ $Last> TupleArgs for ($($T,)+ $Last)
        where
            $($T: std::str::FromStr,)+
            $($T::Err: Into<anyhow::Error>,)+
            $Last: std::str::FromStr,
            $Last::Err: Into<anyhow::Error>,
        {
            type Error = anyhow::Error;
            fn parse_from_iter<'a, I>(iter: &mut I) -> Result<Self, Self::Error>
            where
                I: Iterator<Item = &'a str>,
            {
                let mut idx = 0usize;
                Ok((
                    $({
                        idx += 1;
                        let s = iter.next().ok_or_else(|| anyhow::anyhow!("Missing argument {}", idx))?;
                        s.parse::<$T>().map_err(|e| e.into())?
                    },)+
                    {
                        idx += 1;
                        let s = iter.next().ok_or_else(|| anyhow::anyhow!("Missing argument {}", idx))?;
                        s.parse::<$Last>().map_err(|e| e.into())?
                    }
                ))
            }
        }
    };
}

// Generate implementations for tuples of size 1-16
#[allow(non_snake_case)]
mod tuple_impls {
    use super::*;
    all_the_tuples!(impl_tuple_args);
}

/// Extractor for the rest of the message content
pub struct Rest(pub String);

#[async_trait]
impl<C: MsgContext> FromEvent<C> for Rest {
    type Error = anyhow::Error;

    async fn from_event(ctx: &C) -> Result<Self, Self::Error> {
        let text = ctx.text();
        // Skip command (heuristic)
        let mut parts = text.split_whitespace();
        parts.next(); // skip command

        // Reconstruct the rest
        // Note: split_whitespace loses original whitespace.
        // For distinct "Rest" implementation we might want proper parsing
        // but for now joining valid parts is a reasonable fallback given the vague dispatching
        // Alternatively, use original string index if we knew command length.
        // But we don't know command length here easily.
        // A simple approach: collect the rest of the parts.
        let rest: Vec<&str> = parts.collect();
        Ok(Rest(rest.join(" ")))
    }
}
