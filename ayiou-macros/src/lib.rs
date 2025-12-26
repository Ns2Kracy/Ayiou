use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod derive_plugin;

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
/// AyiouBot::new()
///     .register_plugin(EchoPlugin)
///     .run("ws://...").await;
/// ```
///
/// # Attributes
///
/// - `name`: Plugin name (defaults to struct name in lowercase)
/// - `description`: Plugin description
/// - `version`: Plugin version (defaults to "0.1.0")
/// - `command`: Command that triggers the plugin (defaults to name)
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
