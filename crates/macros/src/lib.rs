use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};

/// Attribute macro for defining tools
///
/// Automatically implements the Tool trait with default methods.
/// See MACROS.md for usage examples.
#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;

    // Generate the struct and impl blocks
    let expanded = quote! {
        #(#attrs)*
        #vis struct #name;

        impl #name {
            pub fn new() -> Self {
                Self
            }
        }

        impl engine_ai_core::Tool for #name {
            fn name(&self) -> &str {
                stringify!(#name)
            }

            fn description(&self) -> &str {
                concat!("Tool: ", stringify!(#name))
            }

            fn execute(&self, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
                Ok(serde_json::json!({}))
            }
        }
    };

    TokenStream::from(expanded)
}

/// Attribute macro for defining providers
///
/// Automatically implements the Provider trait with default methods.
/// See MACROS.md for usage examples.
#[proc_macro_attribute]
pub fn provider(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let fields = &input.fields;

    let expanded = quote! {
        #(#attrs)*
        #vis struct #name #fields

        #[async_trait::async_trait]
        impl engine_ai_core::Provider for #name {
            fn name(&self) -> &str {
                &self.name
            }

            fn driver(&self) -> &str {
                // Should be implemented manually or derived from struct
                "custom"
            }

            fn models(&self) -> &[String] {
                &self.models
            }

            // execute method should be implemented manually
        }
    };

    TokenStream::from(expanded)
}

/// Macro for creating parser test cases
///
/// See MACROS.md for usage examples.
#[proc_macro]
pub fn parser_test(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();

    let expanded = quote! {
        {
            let input = #input_str;
            engine_ai_core::parse_document(input)
        }
    };

    TokenStream::from(expanded)
}

/// Helper macro for defining tool implementations
///
/// See MACROS.md for usage examples.
#[proc_macro]
pub fn define_tool(input: TokenStream) -> TokenStream {
    // For now, just return the input as-is
    // A full implementation would parse the macro syntax and generate the struct
    input
}

