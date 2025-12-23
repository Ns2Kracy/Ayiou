use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, Ident, ItemFn, Pat, PatType, Result, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

/// Attributes for #[command(...)]
#[derive(Default)]
pub struct PluginAttrs {
    pub name: Option<String>,
    pub description: Option<String>,
    pub prefix: Option<String>,
    pub alias: Option<String>,
    pub aliases: Vec<String>,
}

impl Parse for PluginAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = PluginAttrs::default();

        let pairs: Punctuated<syn::MetaNameValue, Token![,]> = Punctuated::parse_terminated(input)?;

        for pair in pairs {
            let key = pair
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new(pair.path.span(), "expected identifier"))?
                .to_string();

            let value = match &pair.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => s.value(),
                _ => {
                    return Err(syn::Error::new(
                        pair.value.span(),
                        "expected string literal",
                    ));
                }
            };

            match key.as_str() {
                "name" => attrs.name = Some(value),
                "description" => attrs.description = Some(value),
                "prefix" => attrs.prefix = Some(value),
                "alias" => attrs.alias = Some(value),
                "aliases" => {
                    attrs.aliases = value.split(',').map(|s| s.trim().to_string()).collect()
                }
                _ => {
                    return Err(syn::Error::new(
                        pair.path.span(),
                        format!("unknown attribute: {}", key),
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

/// Field info extracted from function parameter
struct FieldInfo {
    name: Ident,
    ty: Type,
    is_rest: bool,
    is_optional: bool,
    is_cron: bool,
    regex: Option<String>,
    error: Option<String>,
}

/// Check if type matches any generic parameter
fn is_generic_type(ty: &Type, generics: &syn::Generics) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        for param in &generics.params {
            if let syn::GenericParam::Type(type_param) = param
                && type_param.ident == segment.ident
            {
                return true;
            }
        }
    }
    false
}

/// Check if type is Ctx (legacy or generic)
fn is_context_type(ty: &Type, generics: &syn::Generics) -> bool {
    // Check legacy "Ctx" name
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Ctx"
    {
        return true;
    }
    // Check if it's a generic parameter
    is_generic_type(ty, generics)
}

/// Check if type is Box<T>
fn is_box_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Box";
    }
    false
}

/// Parse parameter attributes like #[rest], #[optional], #[regex("...")]
fn parse_param_attrs(attrs: &[Attribute]) -> (bool, bool, bool, Option<String>, Option<String>) {
    let mut is_rest = false;
    let mut is_optional = false;
    let mut is_cron = false;
    let mut regex = None;
    let mut error = None;

    for attr in attrs {
        if attr.path().is_ident("rest") {
            is_rest = true;
        } else if attr.path().is_ident("optional") {
            is_optional = true;
        } else if attr.path().is_ident("cron") {
            is_cron = true;
        } else if attr.path().is_ident("regex") {
            if let syn::Meta::List(meta_list) = &attr.meta
                && let Ok(lit) = meta_list.parse_args::<syn::LitStr>()
            {
                regex = Some(lit.value());
            }
        } else if attr.path().is_ident("error")
            && let syn::Meta::List(meta_list) = &attr.meta
            && let Ok(lit) = meta_list.parse_args::<syn::LitStr>()
        {
            error = Some(lit.value());
        }
    }

    (is_rest, is_optional, is_cron, regex, error)
}

/// Extract fields from function parameters and identify context type
fn extract_fields(func: &ItemFn) -> Result<(Vec<FieldInfo>, Type)> {
    let mut fields = Vec::new();
    let mut ctx_type = None;

    // Default context type (backward compatibility)
    let default_ctx: Type = syn::parse_str("ayiou::adapter::onebot::v11::ctx::Ctx").unwrap();

    for arg in &func.sig.inputs {
        if let FnArg::Typed(PatType { pat, ty, attrs, .. }) = arg {
            // Check if this is a context parameter
            // We consider it context if it matches "Ctx" or a generic param
            if is_context_type(ty, &func.sig.generics) {
                if ctx_type.is_some() {
                    return Err(syn::Error::new(
                        pat.span(),
                        "multiple context parameters found",
                    ));
                }
                ctx_type = Some(ty.as_ref().clone());
                continue;
            }

            let name = if let Pat::Ident(pat_ident) = pat.as_ref() {
                pat_ident.ident.clone()
            } else {
                return Err(syn::Error::new(pat.span(), "expected identifier pattern"));
            };

            let (is_rest, is_optional, is_cron, regex, error) = parse_param_attrs(attrs);

            fields.push(FieldInfo {
                name,
                ty: ty.as_ref().clone(),
                is_rest,
                is_optional,
                is_cron,
                regex,
                error,
            });
        }
    }

    Ok((fields, ctx_type.unwrap_or(default_ctx)))
}

/// Generate struct name from function name (snake_case -> PascalCase + "Command")
fn generate_struct_name(fn_name: &Ident) -> Ident {
    let name = fn_name.to_string();
    let pascal = name
        .split('_')
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();
    format_ident!("{}Command", pascal)
}

/// Strip custom attributes from function parameters
fn strip_param_attrs(func: &ItemFn) -> ItemFn {
    let mut func = func.clone();
    for arg in &mut func.sig.inputs {
        if let FnArg::Typed(pat_type) = arg {
            // Remove our custom attributes (rest, optional, cron, regex, error)
            pat_type.attrs.retain(|attr| {
                !attr.path().is_ident("rest")
                    && !attr.path().is_ident("optional")
                    && !attr.path().is_ident("cron")
                    && !attr.path().is_ident("regex")
                    && !attr.path().is_ident("error")
            });
        }
    }
    func
}

/// Main expansion function
pub fn expand_plugin(attrs: PluginAttrs, func: ItemFn) -> Result<TokenStream> {
    let fn_name = &func.sig.ident;
    let fn_vis = &func.vis;
    let struct_name = generate_struct_name(fn_name);

    // Extract fields and context type
    let (fields, ctx_type) = extract_fields(&func)?;

    // Strip custom attributes from the function for output
    let clean_func = strip_param_attrs(&func);

    // Command name (default to function name)
    let command_name = attrs.name.unwrap_or_else(|| fn_name.to_string());
    let prefix = attrs.prefix.unwrap_or_else(|| "/".to_string());
    let full_cmd = format!("{}{}", prefix, command_name);
    let description = attrs.description.unwrap_or_default();

    // Generate struct fields
    let struct_fields = fields.iter().map(|f| {
        let name = &f.name;
        let ty = &f.ty;
        quote! { pub #name: #ty }
    });

    // Generate Args::parse implementation
    let args_parse = generate_args_parse(&fields);

    // Generate function call arguments
    let call_args = fields.iter().map(|f| {
        let name = &f.name;
        quote! { self.#name }
    });

    // Generate commands method
    let aliases: Vec<String> = attrs
        .aliases
        .iter()
        .chain(attrs.alias.iter())
        .map(|a| format!("{}{}", prefix, a))
        .collect();

    let commands_method = if aliases.is_empty() {
        quote! {
            fn commands(&self) -> Vec<String> {
                vec![#full_cmd.to_string()]
            }
        }
    } else {
        quote! {
            fn commands(&self) -> Vec<String> {
                vec![#full_cmd.to_string(), #(#aliases.to_string()),*]
            }
        }
    };

    // Use a private inner function name
    let inner_fn_name = format_ident!("__{}_inner", fn_name);

    // Rename the original function to inner
    let mut inner_func = clean_func.clone();
    inner_func.sig.ident = inner_fn_name.clone();

    // Split generics for impl block
    let (impl_generics, _ty_generics, where_clause) = func.sig.generics.split_for_impl();

    // Generate the output
    let output = quote! {
        // The original function renamed to inner (private)
        #[doc(hidden)]
        #inner_func

        // Generate struct with same name as function (PascalCase)
        // Users can use this for type-based registration: plugin::<Echo>()
        #[derive(Default)]
        #fn_vis struct #struct_name {
            #(#struct_fields),*
        }

        // Also create a function that returns the plugin instance
        // Users can use this for value-based registration: plugin(echo())
        #fn_vis fn #fn_name() -> #struct_name {
            #struct_name::default()
        }

        // Implement Args
        impl ayiou::core::Args for #struct_name {
            fn parse(args: &str) -> std::result::Result<Self, ayiou::core::ArgsParseError> {
                let parts: Vec<&str> = args.split_whitespace().collect();
                #args_parse
            }
        }

        // Implement Command
        #[async_trait::async_trait]
        impl #impl_generics ayiou::core::Command<#ctx_type> for #struct_name #where_clause {
            async fn run(self, ctx: #ctx_type) -> anyhow::Result<()> {
                #inner_fn_name(ctx, #(#call_args),*).await
            }
        }

        // Implement Plugin
        #[async_trait::async_trait]
        impl #impl_generics ayiou::core::Plugin<#ctx_type> for #struct_name #where_clause {
            fn meta(&self) -> ayiou::core::PluginMetadata {
                ayiou::core::PluginMetadata::new(stringify!(#struct_name))
                    .description(#description)
            }

            #commands_method

            fn matches(&self, ctx: &#ctx_type) -> bool {
                let text = ctx.text();
                let text = text.trim();
                let (cmd, _) = text.split_once(char::is_whitespace)
                    .map(|(c, a)| (c, a.trim()))
                    .unwrap_or((text, ""));
                #full_cmd == cmd || self.commands().iter().any(|c| c == cmd)
            }

            async fn handle(&self, ctx: &#ctx_type) -> anyhow::Result<bool> {
                let text = ctx.text();
                let text = text.trim();
                let (_, args) = text.split_once(char::is_whitespace)
                    .map(|(c, a)| (c, a.trim()))
                    .unwrap_or((text, ""));

                let parsed = match <Self as ayiou::core::Args>::parse(args) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        let msg = if let Some(help) = e.help() {
                            format!("❌ {}\n\n{}", e.message(), help)
                        } else {
                            format!("❌ {}", e.message())
                        };
                        ctx.reply_text(msg).await?;
                        return Ok(true);
                    }
                };

                parsed.run(ctx.clone()).await?;
                Ok(true)
            }
        }
    };

    Ok(output)
}

/// Generate Args::parse body based on fields
fn generate_args_parse(fields: &[FieldInfo]) -> TokenStream {
    if fields.is_empty() {
        return quote! { Ok(Self {}) };
    }

    let field_count = fields.len();
    let assignments: Vec<TokenStream> = fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let name = &field.name;

            if field.is_rest {
                if i == 0 {
                    quote! { #name: args.trim().to_string() }
                } else {
                    quote! { #name: parts[#i..].join(" ") }
                }
            } else if field.is_cron {
                let error_msg = field
                    .error
                    .as_deref()
                    .unwrap_or("Invalid cron expression");
                let needs_box = is_box_type(&field.ty);
                if field_count == 1 {
                    if needs_box {
                        quote! {
                            #name: Box::new(ayiou::core::CronSchedule::parse(args.trim())
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                        }
                    } else {
                        quote! {
                            #name: ayiou::core::CronSchedule::parse(args.trim())
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    }
                } else if needs_box {
                    quote! {
                        #name: Box::new(ayiou::core::CronSchedule::parse(
                            parts.get(#i).copied().unwrap_or("")
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                    }
                } else {
                    quote! {
                        #name: ayiou::core::CronSchedule::parse(
                            parts.get(#i).copied().unwrap_or("")
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    }
                }
            } else if let Some(ref pattern) = field.regex {
                let error_msg = field.error.clone().unwrap_or_else(|| {
                    format!("Field '{}' does not match pattern '{}'", name, pattern)
                });
                let needs_box = is_box_type(&field.ty);
                if field_count == 1 {
                    if needs_box {
                        quote! {
                            #name: Box::new(ayiou::core::RegexValidated::validate(args.trim(), #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                        }
                    } else {
                        quote! {
                            #name: ayiou::core::RegexValidated::validate(args.trim(), #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    }
                } else if needs_box {
                    quote! {
                        #name: Box::new(ayiou::core::RegexValidated::validate(
                            parts.get(#i).copied().unwrap_or(""), #pattern
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                    }
                } else {
                    quote! {
                        #name: ayiou::core::RegexValidated::validate(
                            parts.get(#i).copied().unwrap_or(""), #pattern
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    }
                }
            } else if field.is_optional {
                if field_count == 1 {
                    quote! {
                        #name: {
                            let value = args.trim();
                            if value.is_empty() { None } else {
                                Some(value.parse().map_err(|e| ayiou::core::ArgsParseError::new(
                                    format!("Invalid argument {}: {}", stringify!(#name), e)
                                ))?)
                            }
                        }
                    }
                } else {
                    quote! {
                        #name: match parts.get(#i) {
                            Some(s) if !s.is_empty() => Some(s.parse().map_err(|e|
                                ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#name), e))
                            )?),
                            _ => None,
                        }
                    }
                }
            } else {
                // Regular field
                if field_count == 1 {
                    quote! {
                        #name: args.trim().parse().map_err(|e|
                            ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#name), e))
                        )?
                    }
                } else {
                    quote! {
                        #name: parts.get(#i)
                            .ok_or_else(|| ayiou::core::ArgsParseError::new(
                                format!("Missing argument: {}", stringify!(#name))
                            ))?
                            .parse()
                            .map_err(|e| ayiou::core::ArgsParseError::new(
                                format!("Invalid argument {}: {}", stringify!(#name), e)
                            ))?
                    }
                }
            }
        })
        .collect();

    quote! {
        Ok(Self {
            #(#assignments),*
        })
    }
}
