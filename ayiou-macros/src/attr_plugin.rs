use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Expr, ExprArray, ExprLit, FnArg, GenericArgument, ImplItem, ItemImpl, Lit, Meta, MetaNameValue,
    Pat, PatIdent, PathArguments, Result, Type,
};

#[derive(Default)]
struct BotPluginAttrs {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    prefixes: Vec<String>,
    context: Option<Type>,
}

#[derive(Default)]
struct CommandAttrs {
    name: Option<String>,
    aliases: Vec<String>,
}

struct CommandMethod {
    fn_name: syn::Ident,
    labels: Vec<String>,
    parser_stmts: Vec<TokenStream>,
    call_args: Vec<syn::Ident>,
}

pub fn expand_bot_plugin(args: Vec<Meta>, mut item_impl: ItemImpl) -> Result<TokenStream> {
    let plugin_attrs = parse_bot_plugin_attrs(args)?;

    let plugin_ty = item_impl.self_ty.clone();
    let plugin_ident = extract_self_type_ident(&plugin_ty)?;

    let ctx_ty = plugin_attrs
        .context
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(ayiou::prelude::Ctx));

    let mut methods = Vec::new();

    for impl_item in &mut item_impl.items {
        let ImplItem::Fn(method) = impl_item else {
            continue;
        };

        let mut command_attr_index = None;
        for (idx, attr) in method.attrs.iter().enumerate() {
            if attr.path().is_ident("command") {
                command_attr_index = Some(idx);
                break;
            }
        }

        let Some(attr_index) = command_attr_index else {
            continue;
        };

        if method.sig.asyncness.is_none() {
            return Err(syn::Error::new_spanned(
                &method.sig,
                "#[command] method must be async",
            ));
        }

        let command_attr = method.attrs.remove(attr_index);
        let cmd_attrs = parse_command_attr(&command_attr)?;

        methods.push(parse_command_method(method, cmd_attrs, &ctx_ty)?);
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl,
            "#[bot_plugin] requires at least one #[command] method",
        ));
    }

    let plugin_name = plugin_attrs
        .name
        .unwrap_or_else(|| plugin_ident.to_string().to_lowercase());
    let plugin_description = plugin_attrs.description.unwrap_or_default();
    let plugin_version = plugin_attrs.version.unwrap_or_else(|| "0.1.0".to_string());

    let all_commands: Vec<String> = methods
        .iter()
        .flat_map(|method| method.labels.iter().cloned())
        .collect();

    let command_values = all_commands
        .iter()
        .map(|value| quote! { #value.to_string() });
    let prefix_values = plugin_attrs
        .prefixes
        .iter()
        .map(|value| quote! { #value.to_string() });

    let dispatch_arms = methods.iter().map(|method| {
        let fn_name = &method.fn_name;
        let labels = method.labels.iter();
        let parser_stmts = &method.parser_stmts;
        let call_args = &method.call_args;

        quote! {
            #(#labels)|* => {
                #(#parser_stmts)*
                self.#fn_name(ctx, #(#call_args),*).await?;
                Ok(true)
            }
        }
    });

    let plugin_impl = quote! {
        #item_impl

        #[async_trait::async_trait]
        impl ayiou::core::plugin::Plugin<#ctx_ty> for #plugin_ty {
            fn meta(&self) -> ayiou::core::plugin::PluginMetadata {
                ayiou::core::plugin::PluginMetadata {
                    name: #plugin_name.to_string(),
                    description: #plugin_description.to_string(),
                    version: #plugin_version.to_string(),
                }
            }

            fn commands(&self) -> Vec<String> {
                vec![#(#command_values),*]
            }

            fn command_prefixes(&self) -> Vec<String> {
                vec![#(#prefix_values),*]
            }

            async fn handle(&self, ctx: &#ctx_ty) -> anyhow::Result<bool> {
                use ayiou::core::adapter::MsgContext;

                let text = ctx.text();
                let mut prefixes_owned = vec!["/".to_string(), "!".to_string(), ".".to_string()];
                prefixes_owned.extend(self.command_prefixes());
                let prefix_refs: Vec<&str> = prefixes_owned.iter().map(String::as_str).collect();

                if let Some(line) = ayiou::core::plugin::parse_command_line(&text, &prefix_refs) {
                    return self.__ayiou_dispatch_command(ctx, line.command(), line.args()).await;
                }

                Ok(false)
            }
        }

        impl #plugin_ty {
            async fn __ayiou_dispatch_command(
                &self,
                ctx: &#ctx_ty,
                command: &str,
                args: &str,
            ) -> anyhow::Result<bool> {
                match command {
                    #(#dispatch_arms,)*
                    _ => Ok(false),
                }
            }
        }
    };

    Ok(plugin_impl)
}

fn parse_bot_plugin_attrs(args: Vec<Meta>) -> Result<BotPluginAttrs> {
    let mut out = BotPluginAttrs::default();

    for meta in args {
        match meta {
            Meta::NameValue(MetaNameValue { path, value, .. }) => {
                let key = path
                    .get_ident()
                    .map(|ident| ident.to_string())
                    .ok_or_else(|| syn::Error::new_spanned(path, "Unsupported bot_plugin key"))?;

                match key.as_str() {
                    "name" => out.name = Some(expect_string_expr(value)?),
                    "description" => out.description = Some(expect_string_expr(value)?),
                    "version" => out.version = Some(expect_string_expr(value)?),
                    "prefix" => out.prefixes.push(expect_string_expr(value)?),
                    "context" => {
                        let ty_str = expect_string_expr(value)?;
                        out.context = Some(syn::parse_str(&ty_str)?);
                    }
                    _ => {
                        return Err(syn::Error::new(
                            Span::call_site(),
                            format!("Unsupported bot_plugin key `{}`", key),
                        ));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unsupported bot_plugin attribute format",
                ));
            }
        }
    }

    Ok(out)
}

fn parse_command_attr(attr: &syn::Attribute) -> Result<CommandAttrs> {
    let mut out = CommandAttrs::default();

    if matches!(&attr.meta, Meta::Path(_)) {
        return Ok(out);
    }

    attr.parse_nested_meta(|meta| {
        let key = meta
            .path
            .get_ident()
            .map(|ident| ident.to_string())
            .ok_or_else(|| syn::Error::new_spanned(&meta.path, "Unsupported command key"))?;

        match key.as_str() {
            "name" => {
                let value: Expr = meta.value()?.parse()?;
                out.name = Some(expect_string_expr(value)?);
            }
            "alias" => {
                let value: Expr = meta.value()?.parse()?;
                out.aliases.push(expect_string_expr(value)?);
            }
            "aliases" => {
                let value: Expr = meta.value()?.parse()?;
                out.aliases.extend(expect_string_array_expr(value)?);
            }
            _ => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("Unsupported command key `{}`", key),
                ));
            }
        }

        Ok(())
    })?;

    Ok(out)
}

fn parse_command_method(
    method: &syn::ImplItemFn,
    attrs: CommandAttrs,
    _ctx_ty: &Type,
) -> Result<CommandMethod> {
    let fn_name = method.sig.ident.clone();
    let mut labels = vec![attrs.name.unwrap_or_else(|| fn_name.to_string())];
    labels.extend(attrs.aliases);

    let mut inputs = method.sig.inputs.iter();

    match inputs.next() {
        Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() => {}
        _ => {
            return Err(syn::Error::new_spanned(
                &method.sig.inputs,
                "#[command] method must start with `&self`",
            ));
        }
    }

    let Some(FnArg::Typed(ctx_arg)) = inputs.next() else {
        return Err(syn::Error::new_spanned(
            &method.sig.inputs,
            "#[command] method must include context as the second argument",
        ));
    };

    match &*ctx_arg.ty {
        Type::Reference(_) => {}
        _ => {
            return Err(syn::Error::new_spanned(
                &ctx_arg.ty,
                "second argument must be a reference context type",
            ));
        }
    }

    let args_inputs: Vec<_> = inputs.collect();

    let mut parser_stmts = vec![quote! {
        let __ayiou_tokens = ayiou::core::plugin::tokenize_command_args(args)?;
        let mut __ayiou_index = 0usize;
    }];

    let mut call_args = Vec::new();

    for (idx, arg) in args_inputs.iter().enumerate() {
        let FnArg::Typed(pat_type) = arg else {
            return Err(syn::Error::new_spanned(arg, "Invalid command argument"));
        };

        let Pat::Ident(PatIdent { ident, .. }) = pat_type.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "argument pattern must be an identifier",
            ));
        };

        let var = ident.clone();
        let arg_name = ident.to_string();
        let ty = &pat_type.ty;
        let is_last = idx == args_inputs.len() - 1;

        if let Some(inner_ty) = unwrap_option_type(ty) {
            parser_stmts.push(quote! {
                let #var = if __ayiou_index < __ayiou_tokens.len() {
                    Some(ayiou::core::plugin::parse_typed_arg::<#inner_ty>(
                        &__ayiou_tokens,
                        &mut __ayiou_index,
                        #arg_name,
                    )?)
                } else {
                    None
                };
            });
        } else if is_last && is_string_type(ty) {
            parser_stmts.push(quote! {
                let #var = __ayiou_tokens[__ayiou_index..].join(" ");
                __ayiou_index = __ayiou_tokens.len();
            });
        } else if is_last && is_vec_string_type(ty) {
            parser_stmts.push(quote! {
                let #var = __ayiou_tokens[__ayiou_index..].to_vec();
                __ayiou_index = __ayiou_tokens.len();
            });
        } else {
            parser_stmts.push(quote! {
                let #var = ayiou::core::plugin::parse_typed_arg::<#ty>(
                    &__ayiou_tokens,
                    &mut __ayiou_index,
                    #arg_name,
                )?;
            });
        }

        call_args.push(var);
    }

    parser_stmts.push(quote! {
        ayiou::core::plugin::ensure_no_extra_args(&__ayiou_tokens, __ayiou_index)?;
    });

    Ok(CommandMethod {
        fn_name,
        labels,
        parser_stmts,
        call_args,
    })
}

fn expect_string_expr(value: Expr) -> Result<String> {
    if let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = value
    {
        Ok(value.value())
    } else {
        Err(syn::Error::new_spanned(value, "Expected string literal"))
    }
}

fn expect_string_array_expr(value: Expr) -> Result<Vec<String>> {
    let Expr::Array(ExprArray { elems, .. }) = value else {
        return Err(syn::Error::new_spanned(
            value,
            "Expected string array literal",
        ));
    };

    let mut out = Vec::with_capacity(elems.len());
    for expr in elems {
        out.push(expect_string_expr(expr)?);
    }

    Ok(out)
}

fn extract_self_type_ident(plugin_ty: &Type) -> Result<syn::Ident> {
    if let Type::Path(path) = plugin_ty
        && let Some(seg) = path.path.segments.last()
    {
        return Ok(seg.ident.clone());
    }

    Err(syn::Error::new_spanned(
        plugin_ty,
        "Unsupported plugin self type",
    ))
}

fn is_string_type(ty: &Type) -> bool {
    if let Type::Path(path) = ty
        && let Some(seg) = path.path.segments.last()
    {
        return seg.ident == "String";
    }

    false
}

fn is_vec_string_type(ty: &Type) -> bool {
    if let Type::Path(path) = ty
        && let Some(seg) = path.path.segments.last()
        && seg.ident == "Vec"
        && let PathArguments::AngleBracketed(inner) = &seg.arguments
        && inner.args.len() == 1
        && let Some(GenericArgument::Type(inner_ty)) = inner.args.first()
    {
        return is_string_type(inner_ty);
    }

    false
}

fn unwrap_option_type(ty: &Type) -> Option<Type> {
    if let Type::Path(path) = ty
        && let Some(seg) = path.path.segments.last()
        && seg.ident == "Option"
        && let PathArguments::AngleBracketed(inner) = &seg.arguments
        && inner.args.len() == 1
        && let Some(GenericArgument::Type(inner_ty)) = inner.args.first()
    {
        return Some(inner_ty.clone());
    }

    None
}
