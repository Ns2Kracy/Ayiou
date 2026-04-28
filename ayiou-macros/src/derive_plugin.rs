use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Expr, ExprLit, Fields, Lit, Result, spanned::Spanned};

/// Parsed plugin attributes from #[plugin(...)]
#[derive(Default)]
pub struct PluginAttrs {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub commands: Vec<String>,
    pub prefixes: Vec<String>,
    pub context_type: Option<syn::Type>,
    pub regex: Option<String>,
    pub cron: Option<String>,
    pub start_method: Option<syn::Ident>,
    pub handler_method: Option<syn::Ident>,
}

impl PluginAttrs {
    pub fn from_attrs(attrs: &[syn::Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("plugin") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                let key = meta.path.get_ident().map(|i| i.to_string());

                match key.as_deref() {
                    Some("name") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.name = Some(s.value());
                        }
                    }
                    Some("description") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.description = Some(s.value());
                        }
                    }
                    Some("version") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.version = Some(s.value());
                        }
                    }
                    Some("command") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.commands.push(s.value());
                        }
                    }
                    Some("prefix") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.prefixes.push(s.value());
                        }
                    }
                    Some("context") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.context_type = Some(syn::parse_str(&s.value())?);
                        }
                    }
                    Some("regex") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.regex = Some(s.value());
                        }
                    }
                    Some("cron") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.cron = Some(s.value());
                        }
                    }
                    Some("start") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.start_method = Some(syn::parse_str(&s.value())?);
                        }
                    }
                    Some("handler") => {
                        let value: Expr = meta.value()?.parse()?;
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            result.handler_method = Some(syn::parse_str(&s.value())?);
                        }
                    }
                    _ => {}
                }
                Ok(())
            })?;
        }

        Ok(result)
    }
}

pub fn expand_derive_plugin(input: DeriveInput) -> Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Verify it's a struct
    match &input.data {
        Data::Struct(s) => {
            if !matches!(&s.fields, Fields::Unit | Fields::Named(_)) {
                return Err(syn::Error::new(
                    s.fields.span(),
                    "Plugin derive only supports unit structs or structs with named fields",
                ));
            }
        }
        _ => {
            return Err(syn::Error::new(
                input.span(),
                "Plugin can only be derived for structs",
            ));
        }
    }

    let attrs = PluginAttrs::from_attrs(&input.attrs)?;

    let plugin_name = attrs
        .name
        .unwrap_or_else(|| name.to_string().to_lowercase());
    let description = attrs.description.unwrap_or_default();
    let version = attrs.version.unwrap_or_else(|| "0.1.0".to_string());

    // Determine context type FIRST (needed for matches_impl)
    let ctx_type = if let Some(ty) = &attrs.context_type {
        quote! { #ty }
    } else {
        // Default: look for generic C or use Ctx
        if generics
            .params
            .iter()
            .any(|p| matches!(p, syn::GenericParam::Type(t) if t.ident == "C"))
        {
            quote! { C }
        } else {
            quote! { ayiou::prelude::Context }
        }
    };

    // Commands impl
    let commands: Vec<_> = attrs
        .commands
        .iter()
        .map(|c| quote! { #c.to_string() })
        .collect();
    let commands_impl = if commands.is_empty() && attrs.regex.is_none() && attrs.cron.is_none() {
        // Default to name as command if no regex/cron
        quote! { vec![#plugin_name.to_string()] }
    } else if commands.is_empty() {
        // Regex or cron plugin - no commands (wildcard)
        quote! { vec![] }
    } else {
        quote! { vec![#(#commands),*] }
    };

    let prefixes: Vec<_> = attrs
        .prefixes
        .iter()
        .map(|p| quote! { #p.to_string() })
        .collect();
    // Generate cron method if needed
    let cron_method = if let Some(cron_expr) = &attrs.cron {
        quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                /// Get the cron expression for this plugin
                pub fn cron_expression(&self) -> &'static str {
                    #cron_expr
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate regex accessor if needed
    let regex_method = if let Some(regex_pattern) = &attrs.regex {
        quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                /// Get the regex pattern for this plugin
                pub fn regex_pattern(&self) -> &'static str {
                    #regex_pattern
                }

                /// Get the compiled regex for this plugin
                pub fn regex(&self) -> &'static regex::Regex {
                    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
                    RE.get_or_init(|| regex::Regex::new(#regex_pattern).expect("Invalid regex pattern"))
                }
            }
        }
    } else {
        quote! {}
    };

    let start_impl = if let Some(start_method) = &attrs.start_method {
        quote! {
            async fn start(
                &mut self,
                services: ayiou::core::plugin_system::RuntimePluginServices<#ctx_type>,
            ) -> anyhow::Result<()> {
                self.#start_method(services.host).await
            }
        }
    } else {
        quote! {}
    };

    let handle_impl = if let Some(handler_method) = &attrs.handler_method {
        quote! {
            async fn handle(&self, ctx: &#ctx_type) -> anyhow::Result<ayiou::core::plugin_system::HandleOutcome> {
                Ok(ayiou::core::plugin_system::HandleOutcome::from_block(
                    self.#handler_method(ctx).await?
                ))
            }
        }
    } else {
        quote! {
            async fn handle(&self, ctx: &#ctx_type) -> anyhow::Result<ayiou::core::plugin_system::HandleOutcome> {
                self.execute(ctx).await?;
                Ok(ayiou::core::plugin_system::HandleOutcome::block())
            }
        }
    };

    let regex_patterns = attrs
        .regex
        .iter()
        .map(|pattern| quote! { #pattern.to_string() });

    let handler_decl_impl = if attrs.cron.is_some() {
        quote! { Vec::new() }
    } else if attrs.regex.is_some() {
        quote! { vec![ayiou::core::plugin_system::HandlerDecl::message_regex(vec![#(#regex_patterns),*])] }
    } else {
        quote! { vec![ayiou::core::plugin_system::HandlerDecl::message_commands(#commands_impl, Vec::<String>::from([#(#prefixes),*]))] }
    };

    let expanded = quote! {
        #[async_trait::async_trait]
        impl #impl_generics ayiou::core::plugin_system::RuntimePlugin<#ctx_type> for #name #ty_generics #where_clause {
            fn instance_id(&self) -> &str {
                #plugin_name
            }

            fn kind(&self) -> &str {
                #plugin_name
            }

            fn meta(&self) -> ayiou::core::plugin::PluginMetadata {
                ayiou::core::plugin::PluginMetadata {
                    name: #plugin_name.to_string(),
                    description: #description.to_string(),
                    version: #version.to_string(),
                }
            }

            fn declared_handlers(&self) -> Vec<ayiou::core::plugin_system::HandlerDecl> {
                #handler_decl_impl
            }

            #start_impl

            #handle_impl
        }

        #cron_method
        #regex_method
    };

    Ok(expanded)
}
