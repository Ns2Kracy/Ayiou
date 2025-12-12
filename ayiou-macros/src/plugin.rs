use darling::{FromDeriveInput, ast::Style};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Result};

use crate::attrs::{PluginAttrs, RenameRule, VariantAttrs};

pub fn expand_plugin(input: DeriveInput) -> Result<TokenStream> {
    let plugin =
        PluginAttrs::from_derive_input(&input).map_err(|e| syn::Error::new_spanned(&input, e))?;

    let variants =
        plugin.data.as_ref().take_enum().ok_or_else(|| {
            syn::Error::new_spanned(&input, "Plugin can only be derived for enums")
        })?;

    let ctx = GenContext::new(&plugin, variants);
    Ok(ctx.generate())
}

struct GenContext<'a> {
    enum_name: &'a syn::Ident,
    plugin_name: String,
    plugin_description: String,
    plugin_version: String,
    prefix: String,
    rename_rule: RenameRule,
    variants: Vec<&'a VariantAttrs>,
}

impl<'a> GenContext<'a> {
    fn new(plugin: &'a PluginAttrs, variants: Vec<&'a VariantAttrs>) -> Self {
        Self {
            enum_name: &plugin.ident,
            plugin_name: plugin
                .name
                .clone()
                .unwrap_or_else(|| plugin.ident.to_string()),
            plugin_description: plugin.description.clone().unwrap_or_default(),
            plugin_version: plugin.version.clone().unwrap_or_else(|| "0.1.0".into()),
            prefix: plugin.prefix.clone().unwrap_or_else(|| "/".into()),
            rename_rule: plugin.rename_rule.unwrap_or_default(),
            variants,
        }
    }

    fn generate(&self) -> TokenStream {
        let enum_name = self.enum_name;
        let plugin_name = &self.plugin_name;
        let plugin_description = &self.plugin_description;
        let plugin_version = &self.plugin_version;

        let default_impl = self.gen_default();
        let parse_arms = self.gen_parse_arms();
        let handle_arms = self.gen_handle_arms();
        let descriptions = self.gen_descriptions();

        quote! {
            #default_impl

            impl #enum_name {
                pub fn parse(text: &str) -> Option<Self> {
                    let text = text.trim();
                    let (cmd, args) = text.split_once(char::is_whitespace)
                        .map(|(c, a)| (c, a.trim()))
                        .unwrap_or((text, ""));
                    match cmd {
                        #parse_arms
                        _ => None,
                    }
                }

                pub fn descriptions() -> &'static [(&'static str, &'static str)] {
                    &[#descriptions]
                }

                pub fn help_text() -> String {
                    let mut help = String::new();
                    let desc = #plugin_description;
                    if !desc.is_empty() {
                        help.push_str(desc);
                        help.push_str("\n\n");
                    }
                    for (cmd, desc) in Self::descriptions() {
                        help.push_str(&format!("{} - {}\n", cmd, desc));
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
                    Self::parse(&ctx.text()).is_some()
                }

                async fn handle(&self, ctx: &ayiou::adapter::onebot::v11::ctx::Ctx) -> anyhow::Result<bool> {
                    match self {
                        #handle_arms
                    }
                }
            }
        }
    }

    fn gen_default(&self) -> TokenStream {
        let enum_name = self.enum_name;
        let first = &self.variants[0];
        let ident = &first.ident;

        let construction = match first.fields.style {
            Style::Unit => quote! { Self::#ident },
            Style::Tuple => quote! { Self::#ident(Default::default()) },
            Style::Struct => {
                let fields = first.fields.iter().map(|f| &f.ident);
                quote! { Self::#ident { #(#fields: Default::default()),* } }
            }
        };

        quote! {
            impl Default for #enum_name {
                fn default() -> Self { #construction }
            }
        }
    }

    fn gen_parse_arms(&self) -> TokenStream {
        let enum_name = self.enum_name;
        let arms = self.variants.iter().map(|v| {
            let ident = &v.ident;
            let cmd_name = self.command_name(v);
            let cmd = format!("{}{}", self.prefix, cmd_name);

            let aliases: Vec<_> = v
                .aliases
                .iter()
                .chain(v.alias.iter())
                .map(|a| format!("{}{}", self.prefix, a))
                .collect();

            let construction = match v.fields.style {
                Style::Unit => quote! { #enum_name::#ident },
                Style::Tuple if v.fields.len() == 1 => {
                    let ty = &v.fields.fields[0].ty;
                    quote! { #enum_name::#ident(<#ty>::parse(args)) }
                }
                Style::Tuple => quote! { #enum_name::#ident(args.into()) },
                Style::Struct => quote! { #enum_name::#ident { text: args.into() } },
            };

            if aliases.is_empty() {
                quote! { #cmd => Some(#construction), }
            } else {
                quote! { #cmd #(| #aliases)* => Some(#construction), }
            }
        });

        quote! { #(#arms)* }
    }

    fn gen_handle_arms(&self) -> TokenStream {
        let enum_name = self.enum_name;
        let arms = self.variants.iter().map(|v| {
            let ident = &v.ident;
            match v.fields.style {
                Style::Unit => {
                    quote! { #enum_name::#ident => Ok(true), }
                }
                Style::Tuple if v.fields.len() == 1 => {
                    quote! {
                        #enum_name::#ident(inner) => {
                            inner.handle(ctx).await?;
                            Ok(true)
                        }
                    }
                }
                Style::Tuple => {
                    quote! { #enum_name::#ident(..) => Ok(true), }
                }
                Style::Struct => {
                    quote! { #enum_name::#ident { .. } => Ok(true), }
                }
            }
        });

        quote! { #(#arms)* }
    }

    fn gen_descriptions(&self) -> TokenStream {
        let items = self.variants.iter().filter(|v| !v.hide).map(|v| {
            let cmd = format!("{}{}", self.prefix, self.command_name(v));
            let desc = v.description.as_deref().unwrap_or("");
            quote! { (#cmd, #desc) }
        });

        quote! { #(#items),* }
    }

    fn command_name(&self, v: &VariantAttrs) -> String {
        v.rename
            .clone()
            .unwrap_or_else(|| self.rename_rule.apply(&v.ident.to_string()))
    }
}
