use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod attrs;
mod plugin;

use plugin::expand_plugin;

/// Derive macro for defining a plugin as an enum with commands.
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Plugin)]
/// #[plugin(name = "bot", prefix = "/", description = "机器人命令")]
/// pub enum BotCommands {
///     #[command(description = "显示帮助")]
///     Help,
///
///     #[command(description = "ping测试", alias = "p")]
///     Ping,
///
///     #[command(description = "回显消息")]
///     Echo { text: String },
/// }
///
/// impl BotCommands {
///     pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
///         match self {
///             Self::Help => ctx.reply_text(Self::help_text()).await?,
///             Self::Ping => ctx.reply_text("pong!").await?,
///             Self::Echo { text } => ctx.reply_text(format!("Echo: {}", text)).await?,
///         }
///         Ok(())
///     }
/// }
/// ```
#[proc_macro_derive(Plugin, attributes(plugin, command))]
pub fn derive_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_plugin(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
