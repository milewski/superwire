#![feature(proc_macro_span)]

use proc_macro::{Literal, TokenStream, TokenTree};

#[proc_macro]
pub fn workflow(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let Some(first_token) = tokens.next() else {
        return compile_error("workflow! requires inline DSL input");
    };

    let mut combined_span = first_token.span();

    for token in tokens {
        let Some(joined_span) = combined_span.join(token.span()) else {
            return compile_error("workflow! could not recover the exact DSL source text from the macro input");
        };

        combined_span = joined_span;
    }

    let Some(dsl_source) = combined_span.source_text() else {
        return compile_error("workflow! could not recover the exact DSL source text from the macro input");
    };

    let dsl_literal = Literal::string(&dsl_source);

    format!("::engine_ai_core::parser::AstBuilder::new(\"test.ai\".to_string()).parse({dsl_literal}).unwrap()")
        .parse()
        .unwrap_or_else(|_| compile_error("workflow! failed to generate parser invocation"))
}

fn compile_error(message: &str) -> TokenStream {
    let message_literal = Literal::string(message);

    TokenStream::from_iter([
        TokenTree::Ident(proc_macro::Ident::new("compile_error", proc_macro::Span::call_site())),
        TokenTree::Punct(proc_macro::Punct::new('!', proc_macro::Spacing::Alone)),
        TokenTree::Group(proc_macro::Group::new(
            proc_macro::Delimiter::Parenthesis,
            TokenStream::from(TokenTree::Literal(message_literal)),
        )),
    ])
}
