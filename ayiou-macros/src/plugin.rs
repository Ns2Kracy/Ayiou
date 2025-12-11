use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result};

use crate::attrs::{PluginAttrs, RenameRule, VariantAttrs};

struct PluginVariant {
    ident: syn::Ident,
    command_name: String,
    description: Option<String>,
    aliases: Vec<String>,
    fields: Fields,
    hide: bool,
}

pub fn expand_plugin(input: DeriveInput) -> Result<TokenStream> {
    let enum_name = &input.ident;

    let plugin_attrs = PluginAttrs::from_attributes(&input.attrs)?;
    let plugin_name = plugin_attrs.name.unwrap_or_else(|| enum_name.to_string());
    let prefix = plugin_attrs.prefix.unwrap_or_else(|| "/".to_string());
    let rename_rule = plugin_attrs.rename_rule.unwrap_or(RenameRule::Lowercase);
    let plugin_description = plugin_attrs.description.unwrap_or_default();
    let plugin_version = plugin_attrs.version.unwrap_or_else(|| "0.1.0".to_string());

    let Data::Enum(data_enum) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "Plugin can only be derived for enums",
        ));
    };

    let mut variants = Vec::new();

    for variant in &data_enum.variants {
        let var_attrs = VariantAttrs::from_attributes(&variant.attrs)?;

        let command_name = if let Some(rename) = var_attrs.rename {
            rename
        } else {
            rename_rule.apply(&variant.ident.to_string())
        };

        let mut aliases = var_attrs.aliases;
        if let Some(alias) = var_attrs.alias {
            aliases.push(alias);
        }

        variants.push(PluginVariant {
            ident: variant.ident.clone(),
            command_name,
            description: var_attrs.description,
            aliases,
            fields: variant.fields.clone(),
            hide: var_attrs.hide,
        });
    }

    let match_arms = generate_match_arms(&variants, &prefix, enum_name);
    let descriptions_impl = generate_descriptions(&variants, &prefix);
    let commands_list = generate_commands_list(&variants, &prefix);

    let output = quote! {
        impl #enum_name {
            /// Parse command from text, returns the parsed command variant
            pub fn parse(text: &str) -> Option<Self> {
                let text = text.trim();
                let mut parts = text.splitn(2, char::is_whitespace);
                let cmd_part = parts.next()?;
                let args = parts.next().unwrap_or("").trim().to_string();

                #match_arms
            }

            /// Get all command descriptions
            pub fn descriptions() -> Vec<(&'static str, &'static str)> {
                #descriptions_impl
            }

            /// Get all commands (for registering with bot API)
            pub fn commands() -> Vec<(&'static str, &'static str)> {
                #commands_list
            }

            /// Get help text
            pub fn help_text() -> String {
                let mut help = String::new();
                let desc = #plugin_description;
                if !desc.is_empty() {
                    help.push_str(desc);
                    help.push_str("\n\n");
                }
                for (cmd, desc) in Self::descriptions() {
                    help.push_str(cmd);
                    help.push_str(" - ");
                    help.push_str(desc);
                    help.push('\n');
                }
                help.trim_end().to_string()
            }
        }

        #[async_trait::async_trait]
        impl ayiou::core::Plugin for #enum_name {
            fn meta(&self) -> ayiou::core::PluginMetadata {
                ayiou::core::PluginMetadata::new(#plugin_name)
                    .description(#plugin_description)
                    .version(#plugin_version)
            }

            fn matches(&self, ctx: &ayiou::adapter::onebot::v11::ctx::Ctx) -> bool {
                let text = ctx.text();
                Self::parse(&text).is_some()
            }

            async fn handle(&self, ctx: &ayiou::adapter::onebot::v11::ctx::Ctx) -> anyhow::Result<bool> {
                let text = ctx.text();
                if let Some(cmd) = Self::parse(&text) {
                    cmd.execute(ctx).await?;
                    return Ok(true);
                }
                Ok(false)
            }
        }
    };

    Ok(output)
}

fn generate_match_arms(
    variants: &[PluginVariant],
    prefix: &str,
    enum_name: &syn::Ident,
) -> TokenStream {
    let arms: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let ident = &v.ident;
            let cmd = format!("{}{}", prefix, v.command_name);
            let aliases: Vec<String> = v
                .aliases
                .iter()
                .map(|a| format!("{}{}", prefix, a))
                .collect();

            let variant_construction = match &v.fields {
                Fields::Unit => quote! { #enum_name::#ident },
                Fields::Unnamed(_) => {
                    quote! { #enum_name::#ident(args.to_string()) }
                }
                Fields::Named(_) => {
                    quote! { #enum_name::#ident { text: args.to_string() } }
                }
            };

            if aliases.is_empty() {
                quote! {
                    #cmd => Some(#variant_construction),
                }
            } else {
                quote! {
                    #cmd #(| #aliases)* => Some(#variant_construction),
                }
            }
        })
        .collect();

    quote! {
        match cmd_part {
            #(#arms)*
            _ => None,
        }
    }
}

fn generate_descriptions(variants: &[PluginVariant], prefix: &str) -> TokenStream {
    let items: Vec<TokenStream> = variants
        .iter()
        .filter(|v| !v.hide)
        .map(|v| {
            let cmd = format!("{}{}", prefix, v.command_name);
            let desc = v.description.as_deref().unwrap_or("");
            quote! { (#cmd, #desc) }
        })
        .collect();

    quote! {
        vec![#(#items),*]
    }
}

fn generate_commands_list(variants: &[PluginVariant], prefix: &str) -> TokenStream {
    let items: Vec<TokenStream> = variants
        .iter()
        .filter(|v| !v.hide)
        .map(|v| {
            let cmd = format!("{}{}", prefix, v.command_name);
            let desc = v.description.as_deref().unwrap_or("");
            quote! { (#cmd, #desc) }
        })
        .collect();

    quote! {
        vec![#(#items),*]
    }
}
