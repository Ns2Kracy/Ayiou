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
            quote! { ayiou::prelude::Ctx }
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
    let command_prefixes_impl = if prefixes.is_empty() {
        quote! {}
    } else {
        quote! {
            fn command_prefixes(&self) -> Vec<String> {
                vec![#(#prefixes),*]
            }
        }
    };

    // Generate matches() implementation
    let matches_impl = if attrs.regex.is_some() {
        quote! {
            fn matches(&self, ctx: &#ctx_type) -> bool {
                use ayiou::core::adapter::MsgContext;
                self.regex().is_match(&ctx.text())
            }
        }
    } else if attrs.cron.is_some() {
        // Cron plugins need special handling - they match based on schedule, not message
        quote! {
            fn matches(&self, _ctx: &#ctx_type) -> bool {
                // Cron plugins don't match on messages - they're timer-based
                // Return false for message matching; the scheduler will call execute() directly
                false
            }
        }
    } else {
        // Default: no custom matches (use Plugin default)
        quote! {}
    };

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

    let expanded = quote! {
        #[async_trait::async_trait]
        impl #impl_generics ayiou::core::plugin::Plugin<#ctx_type> for #name #ty_generics #where_clause {
            fn meta(&self) -> ayiou::core::plugin::PluginMetadata {
                ayiou::core::plugin::PluginMetadata {
                    name: #plugin_name.to_string(),
                    description: #description.to_string(),
                    version: #version.to_string(),
                }
            }

            fn commands(&self) -> Vec<String> {
                #commands_impl
            }

            #command_prefixes_impl

            #matches_impl

            async fn handle(&self, ctx: &#ctx_type) -> anyhow::Result<bool> {
                self.execute(ctx).await?;
                Ok(true)
            }
        }

        #cron_method
        #regex_method
    };

    Ok(expanded)
}
