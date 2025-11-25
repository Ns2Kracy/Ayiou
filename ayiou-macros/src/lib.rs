use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput};

#[derive(FromDeriveInput, Default)]
#[darling(attributes(event))]
struct EventArgs {
    #[darling(default)]
    to_string: bool,
}

#[proc_macro_derive(Event, attributes(event))]
pub fn event_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let event_args = EventArgs::from_derive_input(&input).unwrap();

    let to_string_impl = if event_args.to_string {
        message_impl(&input.data, name)
    } else {
        quote! {}
    };

    let expanded = quote_spanned! {input.span()=>
        impl From<#name> for crate::core::event::Event {
            fn from(event: #name) -> Self {
                Self {
                    name: stringify!(#name).to_string(),
                    data: std::sync::Arc::new(event),
                }
            }
        }

        #to_string_impl
    };

    TokenStream::from(expanded)
}

fn message_impl(data: &Data, name: &syn::Ident) -> proc_macro2::TokenStream {
    let to_string_body = match data {
        Data::Struct(s) => {
            if s.fields.iter().len() == 1 {
                quote! { write!(f, "{}", self.0) }
            } else {
                let fields = s.fields.iter().map(|f| &f.ident);
                quote! { write!(f, "{:?}", (#(#fields),*)) }
            }
        }
        Data::Enum(e) => {
            let variants = e.variants.iter().map(|v| {
                let variant_name = &v.ident;
                let (params, pat) = match &v.fields {
                    syn::Fields::Named(fields) => {
                        let field_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
                        let field_names2 = field_names.clone();
                        (quote!({ #(#field_names),* }), quote!(#(#field_names2),*))
                    }
                    syn::Fields::Unnamed(fields) => {
                        let field_names = (0..fields.unnamed.len()).map(|i| {
                            syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site())
                        });
                        let field_names2 = field_names.clone();
                        (quote!(( #(#field_names),* )), quote!(#(#field_names2),*))
                    }
                    syn::Fields::Unit => (quote!(), quote!("")),
                };
                quote! {
                    #name::#variant_name #params => write!(f, "{}", #pat)
                }
            });
            quote! {
                match self {
                    #(#variants),*
                }
            }
        }
        Data::Union(_) => {
            quote! {Err(std::fmt::Error)}
        }
    };
    quote! {
        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                #to_string_body
            }
        }
    }
}
