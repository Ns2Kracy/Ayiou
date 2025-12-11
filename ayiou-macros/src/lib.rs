use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod args;
mod attrs;
mod plugin;

use args::expand_args;
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
/// pub enum ExamplePlugins {
///     #[plugin(description = "显示帮助")]
///     Help,
///
///     #[plugin(description = "ping测试", alias = "p")]
///     Ping,
///
///     #[plugin(description = "回显消息")]
///     Echo { text: String },
/// }
///
/// #[async_trait]
/// impl ayiou::core::Plugin for ExamplePlugins {
///     async fn handle(&self, ctx: &Ctx) -> anyhow::Result<bool> {
///         if let Some(cmd) = Self::parse(&ctx.text()) {
///             match cmd {
///                 Self::Help => ctx.reply_text(Self::help_text()).await?,
///                 Self::Ping => ctx.reply_text("pong!").await?,
///                 Self::Echo { text } => ctx.reply_text(format!("Echo: {}", text)).await?,
///             }
///             return Ok(true);
///         }
///         Ok(false)
///     }
/// }
///
/// // Register with bot
/// AyiouBot::new().plugin::<ExamplePlugins>().run("ws://...").await;
/// ```
#[proc_macro_derive(Plugin, attributes(plugin))]
pub fn derive_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_plugin(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Derive macro for defining command arguments (similar to clap's Args).
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Args)]
/// pub struct EchoArgs {
///     pub text: String,
/// }
///
/// impl EchoArgs {
///     pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
///         ctx.reply_text(format!("Echo: {}", self.text)).await?;
///         Ok(())
///     }
/// }
/// ```
#[proc_macro_derive(Args, attributes(arg))]
pub fn derive_args(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_args(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
