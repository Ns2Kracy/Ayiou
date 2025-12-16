use darling::{FromDeriveInput, ast::Style};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Result, Type};

use crate::attrs::{PluginAttrs, RenameRule, VariantAttrs};

/// Check if a type is Box<T> and return true if so
fn is_box_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Box";
    }
    false
}

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
        let descriptions = self.gen_descriptions();
        let commands_method = self.gen_commands_method();
        let try_parse_arms = self.gen_try_parse_arms();

        quote! {
            #default_impl

            impl #enum_name {
                /// Check if text matches a command (for matches())
                pub fn matches_cmd(text: &str) -> bool {
                    let text = text.trim();
                    let (cmd, _args) = text.split_once(char::is_whitespace)
                        .map(|(c, a)| (c, a.trim()))
                        .unwrap_or((text, ""));
                    match cmd {
                        #parse_arms
                        _ => false,
                    }
                }

                /// Try to parse text into a command, returning error on args parse failure
                pub fn try_parse(text: &str) -> std::result::Result<Self, ayiou::core::ArgsParseError> {
                    let text = text.trim();
                    let (cmd, args) = text.split_once(char::is_whitespace)
                        .map(|(c, a)| (c, a.trim()))
                        .unwrap_or((text, ""));
                    match cmd {
                        #try_parse_arms
                        _ => Err(ayiou::core::ArgsParseError::new("Unknown command")),
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

                #commands_method

                fn matches(&self, ctx: &ayiou::adapter::onebot::v11::ctx::Ctx) -> bool {
                    Self::matches_cmd(&ctx.text())
                }

                async fn handle(&self, ctx: &ayiou::adapter::onebot::v11::ctx::Ctx) -> anyhow::Result<bool> {
                    // Try to parse the command with args validation
                    let parsed = match Self::try_parse(&ctx.text()) {
                        Ok(cmd) => cmd,
                        Err(e) => {
                            // Auto-reply with error message
                            let msg = if let Some(help) = e.help() {
                                format!("❌ {}\n\n{}", e.message(), help)
                            } else {
                                format!("❌ {}", e.message())
                            };
                            ctx.reply_text(msg).await?;
                            return Ok(true); // Block subsequent handlers
                        }
                    };

                    // Call user-implemented execute method
                    parsed.execute(ctx.clone()).await?;
                    Ok(true)
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

    /// Generate match arms for matches_cmd (returns bool)
    fn gen_parse_arms(&self) -> TokenStream {
        let arms = self.variants.iter().map(|v| {
            let cmd_name = self.command_name(v);
            let cmd = format!("{}{}", self.prefix, cmd_name);

            let aliases: Vec<_> = v
                .aliases
                .iter()
                .chain(v.alias.iter())
                .map(|a| format!("{}{}", self.prefix, a))
                .collect();

            if aliases.is_empty() {
                quote! { #cmd => true, }
            } else {
                quote! { #cmd #(| #aliases)* => true, }
            }
        });

        quote! { #(#arms)* }
    }

    /// Generate match arms for try_parse (returns Result<Self, ArgsParseError>)
    fn gen_try_parse_arms(&self) -> TokenStream {
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
                Style::Unit => quote! { Ok(#enum_name::#ident) },
                Style::Tuple if v.fields.len() == 1 => {
                    let field = &v.fields.fields[0];
                    let ty = &field.ty;

                    // Check for embedded field attributes
                    if field.cron {
                        let error_msg = field.error.as_deref().unwrap_or("Invalid cron expression");
                        quote! {
                            ayiou::core::CronSchedule::parse(args.trim())
                                .map(|inner| #enum_name::#ident(inner))
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))
                        }
                    } else if let Some(ref pattern) = field.regex {
                        let error_msg = field.error.clone().unwrap_or_else(|| {
                            format!("Value does not match pattern '{}'", pattern)
                        });
                        quote! {
                            ayiou::core::RegexValidated::validate(args.trim(), #pattern)
                                .map(|inner| #enum_name::#ident(inner))
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))
                        }
                    } else if field.rest {
                        quote! {
                            Ok(#enum_name::#ident(args.trim().to_string()))
                        }
                    } else {
                        // Fallback to Args trait parse
                        quote! {
                            <#ty as ayiou::core::Args>::parse(args)
                                .map(|inner| #enum_name::#ident(inner))
                        }
                    }
                }
                Style::Tuple => quote! { Ok(#enum_name::#ident(args.into())) },
                Style::Struct => {
                    // Generate inline parsing for struct fields
                    let field_count = v.fields.len();
                    if field_count == 0 {
                        quote! { Ok(#enum_name::#ident {}) }
                    } else {
                        let assignments = self.gen_struct_field_assignments(v);
                        quote! {
                            (|| -> std::result::Result<#enum_name, ayiou::core::ArgsParseError> {
                                let parts: Vec<&str> = args.split_whitespace().collect();
                                Ok(#enum_name::#ident {
                                    #assignments
                                })
                            })()
                        }
                    }
                }
            };

            if aliases.is_empty() {
                quote! { #cmd => #construction, }
            } else {
                quote! { #cmd #(| #aliases)* => #construction, }
            }
        });

        quote! { #(#arms)* }
    }

    /// Generate field assignments for struct variants with embedded parsing
    fn gen_struct_field_assignments(&self, v: &VariantAttrs) -> TokenStream {
        let field_count = v.fields.len();
        let mut has_rest = false;

        let assignments = v.fields.iter().enumerate().map(|(i, field)| {
            let field_name = field.ident.as_ref().unwrap();

            if field.rest {
                has_rest = true;
                if i == 0 {
                    quote! { #field_name: args.trim().to_string() }
                } else {
                    quote! { #field_name: parts[#i..].join(" ") }
                }
            } else if field.cron {
                let error_msg = field.error.as_deref().unwrap_or("Invalid cron expression");
                let needs_box = is_box_type(&field.ty);
                if field_count == 1 {
                    if needs_box {
                        quote! {
                            #field_name: Box::new(ayiou::core::CronSchedule::parse(args.trim())
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                        }
                    } else {
                        quote! {
                            #field_name: ayiou::core::CronSchedule::parse(args.trim())
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    }
                } else if needs_box {
                    quote! {
                        #field_name: Box::new(ayiou::core::CronSchedule::parse(
                            parts.get(#i).copied().unwrap_or("")
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                    }
                } else {
                    quote! {
                        #field_name: ayiou::core::CronSchedule::parse(
                            parts.get(#i).copied().unwrap_or("")
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    }
                }
            } else if let Some(ref pattern) = field.regex {
                let error_msg = field.error.clone().unwrap_or_else(||
                    format!("Field '{}' does not match pattern '{}'", field_name, pattern)
                );
                let needs_box = is_box_type(&field.ty);
                if field_count == 1 {
                    if needs_box {
                        quote! {
                            #field_name: Box::new(ayiou::core::RegexValidated::validate(args.trim(), #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                        }
                    } else {
                        quote! {
                            #field_name: ayiou::core::RegexValidated::validate(args.trim(), #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    }
                } else if needs_box {
                    quote! {
                        #field_name: Box::new(ayiou::core::RegexValidated::validate(
                            parts.get(#i).copied().unwrap_or(""), #pattern
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?)
                    }
                } else {
                    quote! {
                        #field_name: ayiou::core::RegexValidated::validate(
                            parts.get(#i).copied().unwrap_or(""), #pattern
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    }
                }
            } else if field.optional {
                if field_count == 1 {
                    quote! {
                        #field_name: {
                            let value = args.trim();
                            if value.is_empty() { None } else {
                                Some(value.parse().map_err(|e| ayiou::core::ArgsParseError::new(
                                    format!("Invalid argument {}: {}", stringify!(#field_name), e)
                                ))?)
                            }
                        }
                    }
                } else {
                    quote! {
                        #field_name: match parts.get(#i) {
                            Some(s) if !s.is_empty() => Some(s.parse().map_err(|e|
                                ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e))
                            )?),
                            _ => None,
                        }
                    }
                }
            } else {
                // Regular field (use FromStr)
                if field_count == 1 {
                    quote! {
                        #field_name: args.trim().parse().map_err(|e|
                            ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e))
                        )?
                    }
                } else {
                    quote! {
                        #field_name: parts.get(#i)
                            .ok_or_else(|| ayiou::core::ArgsParseError::new(
                                format!("Missing argument: {}", stringify!(#field_name))
                            ))?
                            .parse()
                            .map_err(|e| ayiou::core::ArgsParseError::new(
                                format!("Invalid argument {}: {}", stringify!(#field_name), e)
                            ))?
                    }
                }
            }
        });

        quote! { #(#assignments),* }
    }

    fn gen_descriptions(&self) -> TokenStream {
        let items = self.variants.iter().filter(|v| !v.hide).map(|v| {
            let cmd = format!("{}{}", self.prefix, self.command_name(v));
            let desc = v.description.as_deref().unwrap_or("");
            quote! { (#cmd, #desc) }
        });

        quote! { #(#items),* }
    }

    fn gen_commands_method(&self) -> TokenStream {
        let cmds: Vec<String> = self
            .variants
            .iter()
            .flat_map(|v| {
                let cmd_name = self.command_name(v);
                let cmd = format!("{}{}", self.prefix, cmd_name);
                let mut list = vec![cmd];
                for a in v.aliases.iter().chain(v.alias.iter()) {
                    list.push(format!("{}{}", self.prefix, a));
                }
                list
            })
            .collect();

        quote! {
            fn commands(&self) -> Vec<String> {
                vec![#(#cmds.to_string()),*]
            }
        }
    }

    fn command_name(&self, v: &VariantAttrs) -> String {
        v.rename
            .clone()
            .unwrap_or_else(|| self.rename_rule.apply(&v.ident.to_string()))
    }
}
