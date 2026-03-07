use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ItemStruct, LitStr};

#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let ident = &input.ident;

    TokenStream::from(quote! {
        #input

        impl #ident {
            pub fn macro_generated_tool_name() -> &'static str {
                stringify!(#ident)
            }
        }
    })
}

#[proc_macro_attribute]
pub fn provider(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let ident = &input.ident;

    TokenStream::from(quote! {
        #input

        impl #ident {
            pub fn macro_generated_provider_name() -> &'static str {
                stringify!(#ident)
            }
        }
    })
}

#[proc_macro]
pub fn parser(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as LitStr);
    let value = input.value();

    TokenStream::from(quote! {{
        ::engine_ai_core::parse_workflow(#value)
    }})
}

#[proc_macro_attribute]
pub fn passthrough(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    TokenStream::from(quote! { #input })
}
