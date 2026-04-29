use proc_macro::TokenStream;
use syn::{Meta, parse_macro_input, punctuated::Punctuated};

mod attr_plugin;

use attr_plugin::expand_plugin;

/// Generate a `RuntimePlugin` implementation from an `impl` block.
///
/// `#[plugin]` is the default authoring entrypoint. Put it on an `impl` block and
/// mark command handlers with `#[command]`.
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[derive(Default)]
/// struct EchoPlugin;
///
/// #[plugin(name = "echo", prefix = "/")]
/// impl EchoPlugin {
///     #[command(name = "echo")]
///     async fn echo(&self, ctx: &Context, text: String) -> anyhow::Result<()> {
///         ctx.reply_text(format!("Echo: {}", text)).await
///     }
/// }
/// ```
///
/// # Attributes
///
/// - `name`: Plugin name (defaults to struct name in lowercase)
/// - `description`: Plugin description
/// - `version`: Plugin version (defaults to "0.1.0")
/// - `prefix`: Command prefix accepted by this plugin (repeatable)
/// - `context`: Custom context type (defaults to `Context`)
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr with Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let item_impl = parse_macro_input!(item as syn::ItemImpl);
    expand_plugin(attrs.into_iter().collect(), item_impl)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Mark a method in `#[plugin] impl` as a command handler.
///
/// This attribute is only consumed by `#[plugin]` and is otherwise a no-op.
#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
