use super::*;

#[test]
fn completes_agent_references_inside_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent context_agent {
            model: model.openai_model
            instruction: "hello"
            output {
                value: string
            }
        }

        agent worker {
            model: model.openai_model
            instruction: "example {{ agent.<cursor> }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "context_agent");
    assert_completion_excludes_labels!(&completion_suggestions, "worker");
}

#[test]
fn completes_dynamic_references_inside_tool_input_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        tool format_response {
            input {
                content: string
            }

            output {
                markdown: string
            }
        }

        dynamic {
            source_content: "release notes"
            formatted_result: call tool.format_response {
                input {
                    content: "{{ dynamic.<cursor> }}"
                }
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "source_content");
    assert_completion_excludes_labels!(&completion_suggestions, "formatted_result");
}

#[test]
fn completes_dynamic_root_inside_tool_input_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        tool format_response {
            input {
                content: string
            }

            output {
                markdown: string
            }
        }

        dynamic {
            source_content: "release notes"
            formatted_result: call tool.format_response {
                input {
                    content: "{{ dyn<cursor> }}"
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Dynamic);
}

#[test]
fn suggests_only_agent_and_input_roots_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            customer_name: string
        }

        agent writer {
            instruction: "Write a short welcome message. {{ <cursor> }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ExpressionKeyword::Context,
        ExpressionKeyword::Compact
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        ReferenceKeyword::Tool,
        BuiltinFunctionName::Template,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Provider,
        "string",
        "number"
    );
}

#[test]
fn suppresses_invalid_schema_root_suggestions_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        schema person {
            name: string
        }

        agent writer {
            instruction: "Write a short welcome message. {{ schema.<cursor> }}"
            output {
                value: string
            }
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
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent context_agent {
            model: model.openai_model
            instruction: "hello"
            output {
                value: string
            }
        }

        agent worker {
            model: model.openai_model
            instruction: """
                example {{ agent.<cursor> }}
            """
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "context_agent");
}

#[test]
fn suppresses_suggestions_inside_plain_multiline_prompt_string_text() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent worker {
            model: model.openai_model
            instruction: """
                Like this <cursor>
            """
            output {
                value: string
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_suggestions_inside_plain_single_line_prompt_string_text() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent worker {
            model: model.openai_model
            instruction: "hello <cursor>world"
            output {
                value: string
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn uses_agent_output_field_description_for_interpolation_completion() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greetings {
            output {
                /// some description of the message
                message: string
            }
        }

        agent greetings2 {
            instruction: "Explain: {{ agent.greetings.<cursor> }}."
            output {
                value: string
            }
        }
    };

    let message_completion = completion_suggestion_by_label(&completion_suggestions, "message");

    assert_eq!(message_completion.detail, "some description of the message");
    assert_eq!(message_completion.documentation, "some description of the message");
}

#[test]
fn completes_secrets_references_inside_non_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
        secrets {
            mcp_api_token: string
        }

        mcp local {
            endpoint: secrets.mcp_api_token
            headers {
                Accept: "application/json"
                Authorization: "Bearer {{ secrets.<cursor> }}"
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "mcp_api_token");
}
