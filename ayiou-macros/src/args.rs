use darling::{FromDeriveInput, FromField};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Result};

/// Field-level attributes for #[arg(...)]
#[derive(Debug, FromField)]
#[darling(attributes(arg))]
struct ArgFieldAttrs {
    ident: Option<syn::Ident>,
    ty: syn::Type,

    /// Regex pattern to validate the field value
    #[darling(default)]
    regex: Option<String>,

    /// Mark this field as a cron expression
    #[darling(default)]
    cron: bool,

    /// Mark this field to consume the rest of the input
    #[darling(default)]
    rest: bool,

    /// Mark this field as optional
    #[darling(default)]
    optional: bool,

    /// Custom error message for validation failure
    #[darling(default)]
    error: Option<String>,
}

/// Struct-level attributes for #[derive(Args)]
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(arg), supports(struct_any))]
struct ArgsAttrs {
    ident: syn::Ident,
    data: darling::ast::Data<(), ArgFieldAttrs>,

    /// Usage text for help messages
    #[darling(default)]
    usage: Option<String>,
}

pub fn expand_args(input: DeriveInput) -> Result<TokenStream> {
    let args =
        ArgsAttrs::from_derive_input(&input).map_err(|e| syn::Error::new_spanned(&input, e))?;

    let struct_name = &args.ident;

    let fields =
        args.data.as_ref().take_struct().ok_or_else(|| {
            syn::Error::new_spanned(&input, "Args can only be derived for structs")
        })?;

    let field_count = fields.len();

    // Generate parse implementation based on fields
    let parse_impl = if field_count == 0 {
        // Unit-like struct with no fields
        quote! {
            fn parse(_args: &str) -> std::result::Result<Self, ayiou::core::ArgsParseError> {
                Ok(Self {})
            }
        }
    } else {
        // Generate field assignments
        let mut assignments = Vec::new();
        let mut has_rest = false;

        for (i, field) in fields.iter().enumerate() {
            let field_name = field.ident.as_ref().unwrap();
            let _field_ty = &field.ty; // Reserved for future type-based parsing

            if field.rest {
                has_rest = true;
                // Rest field consumes remaining input
                if i == 0 {
                    assignments.push(quote! {
                        #field_name: args.trim().to_string()
                    });
                } else {
                    assignments.push(quote! {
                        #field_name: parts[#i..].join(" ")
                    });
                }
            } else if field.cron {
                // Cron field - parse as CronSchedule
                let error_msg = field.error.as_deref().unwrap_or("Invalid cron expression");
                if field_count == 1 {
                    assignments.push(quote! {
                        #field_name: ayiou::core::CronSchedule::parse(args.trim())
                            .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    });
                } else {
                    assignments.push(quote! {
                        #field_name: ayiou::core::CronSchedule::parse(
                            parts.get(#i).copied().unwrap_or("")
                        ).map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                    });
                }
            } else if let Some(ref pattern) = field.regex {
                // Regex-validated field
                let error_msg = field.error.clone().unwrap_or_else(|| {
                    format!(
                        "Field '{}' does not match pattern '{}'",
                        field_name, pattern
                    )
                });
                if field_count == 1 {
                    assignments.push(quote! {
                        #field_name: {
                            let value = args.trim();
                            ayiou::core::RegexValidated::validate(value, #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    });
                } else {
                    assignments.push(quote! {
                        #field_name: {
                            let value = parts.get(#i).copied().unwrap_or("");
                            ayiou::core::RegexValidated::validate(value, #pattern)
                                .map_err(|_| ayiou::core::ArgsParseError::new(#error_msg))?
                        }
                    });
                }
            } else if field.optional {
                // Optional field
                if field_count == 1 {
                    assignments.push(quote! {
                        #field_name: {
                            let value = args.trim();
                            if value.is_empty() { None } else {
                                Some(value.parse().map_err(|e| ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e)))?)
                            }
                        }
                    });
                } else {
                    assignments.push(quote! {
                        #field_name: match parts.get(#i) {
                            Some(s) if !s.is_empty() => Some(s.parse().map_err(|e| ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e)))?),
                            _ => None,
                        }
                    });
                }
            } else {
                // Regular field (use FromStr)
                if field_count == 1 {
                    assignments.push(quote! {
                        #field_name: args.trim().parse().map_err(|e| ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e)))?
                    });
                } else {
                    assignments.push(quote! {
                        #field_name: parts.get(#i)
                            .ok_or_else(|| ayiou::core::ArgsParseError::new(format!("Missing argument: {}", stringify!(#field_name))))?
                            .parse()
                            .map_err(|e| ayiou::core::ArgsParseError::new(format!("Invalid argument {}: {}", stringify!(#field_name), e)))?
                    });
                }
            }
        }

        if field_count == 1 || has_rest {
            quote! {
                fn parse(args: &str) -> std::result::Result<Self, ayiou::core::ArgsParseError> {
                    let parts: Vec<&str> = args.split_whitespace().collect();
                    Ok(Self {
                        #(#assignments),*
                    })
                }
            }
        } else {
            quote! {
                fn parse(args: &str) -> std::result::Result<Self, ayiou::core::ArgsParseError> {
                    let parts: Vec<&str> = args.split_whitespace().collect();
                    Ok(Self {
                        #(#assignments),*
                    })
                }
            }
        }
    };

    // Generate usage() implementation
    let usage_impl = if let Some(ref usage) = args.usage {
        quote! {
            fn usage() -> Option<&'static str> {
                Some(#usage)
            }
        }
    } else {
        quote! {}
    };

    let output = quote! {
        impl Default for #struct_name {
            fn default() -> Self {
                Self::parse("").unwrap_or_else(|_| {
                    // Fallback for default - create with empty/default values
                    // This is a best-effort default
                    panic!("Cannot create default {} without valid input", stringify!(#struct_name))
                })
            }
        }

        impl ayiou::core::Args for #struct_name {
            #parse_impl
            #usage_impl
        }
    };

    Ok(output)
}
