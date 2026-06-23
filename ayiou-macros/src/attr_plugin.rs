use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Expr, ExprArray, ExprLit, ExprUnary, FnArg, GenericArgument, ImplItem, ItemImpl, Lit, Meta,
    MetaNameValue, Pat, PatIdent, PathArguments, Result, Type, UnOp,
};

struct PluginAttrs {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    prefixes: Vec<String>,
    register: bool,
}

impl Default for PluginAttrs {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            version: None,
            prefixes: Vec::new(),
            register: true,
        }
    }
}

#[derive(Default)]
struct CommandAttrs {
    name: Option<String>,
    aliases: Vec<String>,
    summary: Option<String>,
    usage: Option<String>,
    examples: Vec<String>,
    priority: Option<i32>,
    block: Option<bool>,
    permissions: Vec<String>,
}

struct CommandMethod {
    fn_name: syn::Ident,
    labels: Vec<String>,
    meta: CommandMetaAttrs,
    parser_stmts: Vec<TokenStream>,
    call_args: Vec<syn::Ident>,
}

struct CommandMetaAttrs {
    command: String,
    aliases: Vec<String>,
    summary: Option<String>,
    usage: Option<String>,
    examples: Vec<String>,
    priority: Option<i32>,
    block: bool,
    permissions: Vec<String>,
}

struct PluginIdentity {
    name: String,
    description: String,
    version: String,
    register: bool,
}

pub fn expand_plugin(args: Vec<Meta>, mut item_impl: ItemImpl) -> Result<TokenStream> {
    let plugin_attrs = parse_plugin_attrs(args)?;
    let plugin_ty = item_impl.self_ty.clone();
    let plugin_ident = extract_self_type_ident(&plugin_ty)?;
    let ctx_ty: Type = syn::parse_quote!(ayiou::Context);
    let methods = collect_command_methods(&mut item_impl)?;
    let identity = plugin_identity(&plugin_attrs, &plugin_ident);

    Ok(render_plugin_impl(
        &item_impl,
        &plugin_ty,
        &ctx_ty,
        identity,
        &plugin_attrs.prefixes,
        &methods,
    ))
}

fn collect_command_methods(item_impl: &mut ItemImpl) -> Result<Vec<CommandMethod>> {
    let mut methods = Vec::new();

    for impl_item in &mut item_impl.items {
        let ImplItem::Fn(method) = impl_item else {
            continue;
        };

        if method.sig.asyncness.is_none() {
            continue;
        }

        let cmd_attrs = if let Some(attr_index) = command_attr_index(method) {
            let command_attr = method.attrs.remove(attr_index);
            parse_command_attr(&command_attr)?
        } else {
            CommandAttrs::default()
        };

        method
            .attrs
            .push(syn::parse_quote!(#[allow(clippy::unused_async)]));

        methods.push(parse_command_method(method, cmd_attrs)?);
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl,
            "#[plugin] requires at least one async command method",
        ));
    }

    Ok(methods)
}

fn command_attr_index(method: &syn::ImplItemFn) -> Option<usize> {
    method
        .attrs
        .iter()
        .position(|attr| attr.path().is_ident("command"))
}

fn plugin_identity(attrs: &PluginAttrs, plugin_ident: &syn::Ident) -> PluginIdentity {
    PluginIdentity {
        name: attrs
            .name
            .clone()
            .unwrap_or_else(|| plugin_ident.to_string().to_lowercase()),
        description: attrs.description.clone().unwrap_or_default(),
        version: attrs.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
        register: attrs.register,
    }
}

fn render_plugin_impl(
    item_impl: &ItemImpl,
    plugin_ty: &Type,
    ctx_ty: &Type,
    identity: PluginIdentity,
    prefixes: &[String],
    methods: &[CommandMethod],
) -> TokenStream {
    let handler_decls = methods.iter().map(|method| {
        let command_values = method
            .labels
            .iter()
            .map(|value| quote! { #value.to_string() });
        let prefix_values = prefixes.iter().map(|value| quote! { #value.to_string() });
        let meta = &method.meta;
        let command = &meta.command;
        let aliases = meta
            .aliases
            .iter()
            .map(|value| quote! { #value.to_string() });
        let summary = meta.summary.as_ref().map(|value| {
            quote! { .summary(#value) }
        });
        let usage = meta.usage.as_ref().map(|value| {
            quote! { .usage(#value) }
        });
        let examples = meta
            .examples
            .iter()
            .map(|value| quote! { #value.to_string() });
        let permissions = meta.permissions.iter().map(|value| {
            quote! { ayiou::core::plugin::Permission::custom(#value) }
        });
        let priority = meta.priority.map(|value| quote! { .priority(#value) });
        let block = meta.block;

        quote! {
            ayiou::core::plugin::HandlerDecl::message_commands(
                vec![#(#command_values),*],
                Vec::<String>::from([#(#prefix_values),*]),
            )
            .command_meta([
                ayiou::core::plugin::CommandMeta::new(#command)
                    .aliases(Vec::<String>::from([#(#aliases),*]))
                    #summary
                    #usage
                    .examples(Vec::<String>::from([#(#examples),*]))
            ])
            .require_permissions(Vec::<ayiou::core::plugin::Permission>::from([#(#permissions),*]))
            #priority
            .block(#block)
        }
    });
    let plugin_name = identity.name;
    let plugin_description = identity.description;
    let plugin_version = identity.version;
    let registration = if identity.register {
        quote! {
                ayiou::inventory::submit! {
                ayiou::core::plugin::PluginRegistration {
                    instance_id: #plugin_name,
                    factory: || -> Box<dyn ayiou::core::plugin::RuntimePlugin> {
                        Box::new(<#plugin_ty as ::std::default::Default>::default())
                    },
                }
            }
        }
    } else {
        quote! {}
    };

    let dispatch_arms = methods.iter().map(|method| {
        let fn_name = &method.fn_name;
        let labels = method.labels.iter();
        let parser_stmts = &method.parser_stmts;
        let call_args = &method.call_args;
        let block = method.meta.block;

        quote! {
            #(#labels)|* => {
                #(#parser_stmts)*
                self.#fn_name(ctx, #(#call_args),*).await?;
                Ok(ayiou::core::plugin::HandleOutcome::from_block(#block))
            }
        }
    });

    quote! {
        #item_impl

        #[async_trait::async_trait]
        impl ayiou::core::plugin::RuntimePlugin for #plugin_ty {
            fn kind(&self) -> &str {
                #plugin_name
            }

            fn manifest(&self) -> ayiou::core::plugin::RuntimePluginManifest {
                ayiou::core::plugin::RuntimePluginManifest::new(#plugin_name)
                    .description(#plugin_description)
                    .version(#plugin_version)
            }

            fn declared_handlers(&self) -> Vec<ayiou::core::plugin::HandlerDecl> {
                vec![#(#handler_decls),*]
            }

            async fn handle_with_invocation(
                &self,
                ctx: &#ctx_ty,
                invocation: Option<ayiou::core::model::CommandInvocation>,
            ) -> anyhow::Result<ayiou::core::plugin::HandleOutcome> {
                let Some(line) = invocation else {
                    return Ok(ayiou::core::plugin::HandleOutcome::pass());
                };
                self.__ayiou_dispatch_command(ctx, line.command(), line.args()).await
            }

            async fn handle(&self, _ctx: &#ctx_ty) -> anyhow::Result<ayiou::core::plugin::HandleOutcome> {
                Ok(ayiou::core::plugin::HandleOutcome::pass())
            }
        }

        impl #plugin_ty {
            async fn __ayiou_dispatch_command(
                &self,
                ctx: &#ctx_ty,
                command: &str,
                args: &str,
            ) -> anyhow::Result<ayiou::core::plugin::HandleOutcome> {
                match command {
                    #(#dispatch_arms,)*
                    _ => Ok(ayiou::core::plugin::HandleOutcome::pass()),
                }
            }
        }

        #registration
    }
}

fn parse_plugin_attrs(args: Vec<Meta>) -> Result<PluginAttrs> {
    let mut out = PluginAttrs::default();

    for meta in args {
        match meta {
            Meta::NameValue(MetaNameValue { path, value, .. }) => {
                let key = path
                    .get_ident()
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| syn::Error::new_spanned(path, "Unsupported plugin key"))?;

                match key.as_str() {
                    "name" => out.name = Some(expect_string_expr(value)?),
                    "description" => out.description = Some(expect_string_expr(value)?),
                    "version" => out.version = Some(expect_string_expr(value)?),
                    "prefix" => out.prefixes.push(expect_string_expr(value)?),
                    "register" => out.register = expect_bool_expr(value)?,
                    _ => {
                        return Err(syn::Error::new(
                            Span::call_site(),
                            format!("Unsupported plugin key `{key}`"),
                        ));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unsupported plugin attribute format",
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
            .map(std::string::ToString::to_string)
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
            "summary" => {
                let value: Expr = meta.value()?.parse()?;
                out.summary = Some(expect_string_expr(value)?);
            }
            "usage" => {
                let value: Expr = meta.value()?.parse()?;
                out.usage = Some(expect_string_expr(value)?);
            }
            "examples" => {
                let value: Expr = meta.value()?.parse()?;
                out.examples.extend(expect_string_array_expr(value)?);
            }
            "priority" => {
                let value: Expr = meta.value()?.parse()?;
                out.priority = Some(expect_i32_expr(value)?);
            }
            "block" => {
                let value: Expr = meta.value()?.parse()?;
                out.block = Some(expect_bool_expr(value)?);
            }
            "permissions" => {
                let value: Expr = meta.value()?.parse()?;
                out.permissions.extend(expect_string_array_expr(value)?);
            }
            _ => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("Unsupported command key `{key}`"),
                ));
            }
        }

        Ok(())
    })?;

    Ok(out)
}

fn parse_command_method(method: &syn::ImplItemFn, attrs: CommandAttrs) -> Result<CommandMethod> {
    let fn_name = method.sig.ident.clone();
    let command = attrs.name.unwrap_or_else(|| fn_name.to_string());
    let labels = std::iter::once(command.clone())
        .chain(attrs.aliases.iter().cloned())
        .collect();
    let meta = CommandMetaAttrs {
        command,
        aliases: attrs.aliases,
        summary: attrs.summary,
        usage: attrs.usage,
        examples: attrs.examples,
        priority: attrs.priority,
        block: attrs.block.unwrap_or(true),
        permissions: attrs.permissions,
    };

    let mut inputs = method.sig.inputs.iter();

    match inputs.next() {
        Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() => {}
        _ => {
            return Err(syn::Error::new_spanned(
                &method.sig.inputs,
                "command method must start with `&self`",
            ));
        }
    }

    let Some(FnArg::Typed(ctx_arg)) = inputs.next() else {
        return Err(syn::Error::new_spanned(
            &method.sig.inputs,
            "command method must include context as the second argument",
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
        let __ayiou_tokens = ayiou::core::command::tokenize_command_args(args)?;
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
                    Some(ayiou::core::command::parse_typed_arg::<#inner_ty>(
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
                let #var = ayiou::core::command::parse_typed_arg::<#ty>(
                    &__ayiou_tokens,
                    &mut __ayiou_index,
                    #arg_name,
                )?;
            });
        }

        call_args.push(var);
    }

    parser_stmts.push(quote! {
        ayiou::core::command::ensure_no_extra_args(&__ayiou_tokens, __ayiou_index)?;
    });

    Ok(CommandMethod {
        fn_name,
        labels,
        meta,
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

fn expect_bool_expr(value: Expr) -> Result<bool> {
    if let Expr::Lit(ExprLit {
        lit: Lit::Bool(value),
        ..
    }) = value
    {
        Ok(value.value)
    } else {
        Err(syn::Error::new_spanned(value, "Expected bool literal"))
    }
}

fn expect_i32_expr(value: Expr) -> Result<i32> {
    match &value {
        Expr::Lit(ExprLit {
            lit: Lit::Int(value),
            ..
        }) => value.base10_parse(),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => {
            if let Expr::Lit(ExprLit {
                lit: Lit::Int(value),
                ..
            }) = expr.as_ref()
            {
                value.base10_parse::<i32>().map(|value| -value)
            } else {
                Err(syn::Error::new_spanned(value, "Expected integer literal"))
            }
        }
        _ => Err(syn::Error::new_spanned(value, "Expected integer literal")),
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
