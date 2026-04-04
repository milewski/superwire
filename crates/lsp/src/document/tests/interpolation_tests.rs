use super::*;

#[test]
fn completes_agent_references_inside_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent context_agent {
            model: openai("gpt-4.1-mini")
            prompt: "hello"
            output: string
        }

        agent worker {
            model: openai("gpt-4.1-mini")
            prompt: "example {{ agent.<cursor> }}"
            output: string
        }
    };

    assert_completion_contains!(&completion_suggestions, "context_agent");
    assert_completion_excludes_labels!(&completion_suggestions, "worker");
}

#[test]
fn suggests_only_agent_and_input_roots_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            customer_name: string
        }

        agent writer {
            prompt: "Write a short welcome message. {{ <cursor> }}"
            output: string
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Agent, ReferenceKeyword::Input);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        ReferenceKeyword::Tool,
        BuiltinFunctionName::Context,
        BuiltinFunctionName::Template,
        BuiltinFunctionName::Compact,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Provider,
        "string",
        "number"
    );
}

#[test]
fn suppresses_invalid_schema_root_suggestions_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            name: string
        }

        agent writer {
            prompt: "Write a short welcome message. {{ schema.<cursor> }}"
            output: string
        }
    };

    let completion_labels = completion_suggestions
        .iter()
        .map(|completion_suggestion| completion_suggestion.label.clone())
        .collect::<Vec<_>>();

    assert!(completion_suggestions.is_empty(), "unexpected suggestions: {completion_labels:?}");
}

#[test]
fn completes_agent_references_inside_multiline_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent context_agent {
            model: openai("gpt-4.1-mini")
            prompt: "hello"
            output: string
        }

        agent worker {
            model: openai("gpt-4.1-mini")
            prompt: """
                example {{ agent.<cursor> }}
            """
            output: string
        }
    };

    assert_completion_contains!(&completion_suggestions, "context_agent");
}

#[test]
fn suppresses_suggestions_inside_plain_multiline_prompt_string_text() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent worker {
            model: openai("gpt-4.1-mini")
            prompt: """
                Like this <cursor>
            """
            output: string
        }
    };

    assert!(completion_suggestions.is_empty());
}
