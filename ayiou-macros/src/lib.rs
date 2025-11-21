use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Marks a struct as a Plugin.
/// In a full version, this would auto-discover handlers.
/// For now, it just derives the name from the struct name.
#[proc_macro_attribute]
pub fn plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // For now, just return the item as is.
    // We expect the user to implement Plugin trait manually or we provide a derive.
    // Let's do a derive instead basically.
    item
}

/// Wraps an async function into a struct that implements EventHandler.
/// Usage:
/// ```rust
/// #[handler]
/// async fn my_handler(ctx: Context, event: Arc<dyn Event>) { ... }
/// ```
/// Generates:
/// ```rust
/// struct MyHandlerStruct;
/// impl EventHandler for MyHandlerStruct { ... }
/// ```
#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let struct_name_ident = syn::Ident::new(&format!("{}Struct", fn_name), fn_name.span());
    let vis = &input_fn.vis;

    let expanded = quote! {
        // The original function
        #input_fn

        // The generated handler struct
        #[derive(Clone)]
        #[allow(non_camel_case_types)]
        #vis struct #struct_name_ident;

        #[async_trait::async_trait]
        impl ayiou::core::event::EventHandler for #struct_name_ident {
            async fn handle(&self, ctx: ayiou::core::Context, event: std::sync::Arc<dyn ayiou::core::Event>) {
                #fn_name(ctx, event).await;
            }
        }
    };

    TokenStream::from(expanded)
}
