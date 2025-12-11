use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod attrs;
mod plugin;

use plugin::expand_plugin;

/// Derive macro for defining bot plugins with commands.
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Plugin)]
/// #[plugin(prefix = "/", description = "可用命令列表:")]
/// pub enum ExamplePlugins{
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
/// impl ExamplePlugins {
///     pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
///         match self {
///             Self::Help => ctx.reply_text(Self::help_text()).await?,
///             Self::Ping => ctx.reply_text("pong!").await?,
///             Self::Echo { text } => ctx.reply_text(format!("Echo: {}", text)).await?,
///         }
///         Ok(())
///     }
/// }
///
/// // Register with bot
/// AyiouBot::new().plugin::<ExamplePlugins>().run("ws://...").await;
/// ```
#[proc_macro_derive(Plugin, attributes(plugin, command))]
pub fn derive_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_plugin(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
