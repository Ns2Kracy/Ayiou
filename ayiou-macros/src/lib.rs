use proc_macro::TokenStream;
use syn::{DeriveInput, Meta, parse_macro_input, punctuated::Punctuated};

mod attr_plugin;
mod derive_plugin;

use attr_plugin::expand_bot_plugin;
use derive_plugin::expand_derive_plugin;

/// Derive macro for simple plugin definition.
///
/// This macro generates `Plugin` trait implementation from struct attributes.
/// You must implement an `execute` method on your struct that handles the command.
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Plugin)]
/// #[plugin(name = "echo", command = "echo", description = "Repeats your message")]
/// struct EchoPlugin;
///
/// impl EchoPlugin {
///     async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
///         let text = ctx.text();
///         ctx.reply_text(format!("Echo: {}", text)).await?;
///         Ok(())
///     }
/// }
///
/// // Register with bot
/// use ayiou::adapter::onebot::v11::adapter::OneBotV11Adapter;
///
/// Bot::<OneBotV11Adapter>::new()
///     .register_plugin(EchoPlugin)
///     .run(OneBotV11Adapter::new("ws://...")).await;
/// ```
///
/// # Attributes
///
/// - `name`: Plugin name (defaults to struct name in lowercase)
/// - `description`: Plugin description
/// - `version`: Plugin version (defaults to "0.1.0")
/// - `command`: Command that triggers the plugin (defaults to name)
/// - `prefix`: Command prefix accepted by this plugin (repeatable)
/// - `regex`: Regex pattern for message matching
/// - `cron`: Cron expression for scheduled execution
/// - `context`: Custom context type (defaults to `Ctx` or generic `C`)
#[proc_macro_derive(Plugin, attributes(plugin))]
pub fn derive_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_plugin(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Mark a method in `#[bot_plugin] impl` as a command handler.
///
/// This attribute is only consumed by `#[bot_plugin]` and is otherwise a no-op.
#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Generate a full `Plugin` implementation from an `impl` block.
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Default)]
/// struct ToolPlugin;
///
/// #[bot_plugin(name = "tool", prefix = "/", prefix = "!")]
/// impl ToolPlugin {
///     #[command(name = "echo", alias = "say")]
///     async fn echo(&self, ctx: &Ctx, content: String) -> anyhow::Result<()> {
///         ctx.reply_text(content).await
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn bot_plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr with Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let item_impl = parse_macro_input!(item as syn::ItemImpl);
    expand_bot_plugin(attrs.into_iter().collect(), item_impl)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
