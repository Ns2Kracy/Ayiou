use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

mod plugin;

use plugin::expand_plugin;

/// Attribute macro for defining command handlers as simple functions.
///
/// This macro transforms an async function into a full plugin, automatically generating:
/// - A struct with function parameters as fields
/// - `Args` trait implementation for argument parsing
/// - `Command` trait implementation that calls the original function
/// - `Plugin` trait implementation for bot registration
///
/// # Example
///
/// ```ignore
/// use ayiou::prelude::*;
///
/// #[plugin(name = "echo", description = "Repeats what you say")]
/// async fn echo(ctx: Ctx, #[rest] content: String) -> Result<()> {
///     ctx.reply_text(format!("Echo: {}", content)).await?;
///     Ok(())
/// }
///
/// #[plugin(name = "add", description = "Adds two numbers")]
/// async fn add(ctx: Ctx, a: i32, b: i32) -> Result<()> {
///     ctx.reply_text(format!("{} + {} = {}", a, b, a + b)).await?;
///     Ok(())
/// }
///
/// // Register with bot - the macro generates EchoCommand and AddCommand structs
/// AyiouBot::new()
///     .plugin::<EchoCommand>()
///     .plugin::<AddCommand>()
///     .run("ws://...").await;
/// ```
///
/// # Attributes
///
/// - `name`: Command name (defaults to function name)
/// - `description`: Command description
/// - `prefix`: Command prefix (defaults to "/")
/// - `alias`: Single alias for the command
/// - `aliases`: Comma-separated list of aliases
///
/// # Parameter Attributes
///
/// - `#[rest]`: Consume the rest of the input as a single string
/// - `#[optional]`: Make the parameter optional (wrap in Option<T>)
/// - `#[cron]`: Parse as cron expression
/// - `#[regex("pattern")]`: Validate against regex pattern
/// - `#[error("message")]`: Custom error message for validation failure
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr);
    let func = parse_macro_input!(item as ItemFn);
    expand_plugin(attrs, func)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
