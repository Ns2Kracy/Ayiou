use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result};

pub fn expand_args(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    let fields_info = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                let field_count = field_names.len();

                if field_count == 0 {
                    quote! {
                        impl #struct_name {
                            pub fn parse(_args: &str) -> Self {
                                Self {}
                            }
                        }
                    }
                } else if field_count == 1 {
                    let field_name = &field_names[0];
                    quote! {
                        impl #struct_name {
                            pub fn parse(args: &str) -> Self {
                                Self {
                                    #field_name: args.to_string(),
                                }
                            }
                        }
                    }
                } else {
                    let assignments: Vec<_> = field_names
                        .iter()
                        .enumerate()
                        .map(|(i, name)| {
                            quote! {
                                #name: parts.get(#i).map(|s| s.to_string()).unwrap_or_default()
                            }
                        })
                        .collect();

                    quote! {
                        impl #struct_name {
                            pub fn parse(args: &str) -> Self {
                                let parts: Vec<&str> = args.split_whitespace().collect();
                                Self {
                                    #(#assignments),*
                                }
                            }
                        }
                    }
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    impl #struct_name {
                        pub fn parse(args: &str) -> Self {
                            Self(args.to_string())
                        }
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    impl #struct_name {
                        pub fn parse(_args: &str) -> Self {
                            Self
                        }
                    }
                }
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "Args can only be derived for structs",
            ));
        }
    };

    let output = quote! {
        impl Default for #struct_name {
            fn default() -> Self {
                Self::parse("")
            }
        }

        #fields_info
    };

    Ok(output)
}
