use super::*;

#[test]
fn completes_nested_input_field_attributes() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            profile: {
                name: {
                    first: string
                    last: string
                }
            }
        }

        output {
            value: input.profile.name.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "first", "last");
}

#[test]
fn completes_provider_driver_specific_properties() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {
            <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "endpoint", "api_key");
}

#[test]
fn suppresses_suggestions_inside_model_id_string_literal() {
    let completion_suggestions = inline_completion_suggestions! {
        provider ollama from ollama {}

        model ollama_model from ollama {
            id: "<cursor>"
        }
    };

    assert_completion_excludes_labels!(&completion_suggestions, "[]", "[\"\"]");
}

#[test]
fn suppresses_builtin_functions_in_top_level_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        <cursor>

        output {
            value: null
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Tool,
        SingletonDeclarationKind::Input,
        SingletonDeclarationKind::Secrets
    );

    assert_completion_excludes_labels!(&completion_suggestions, SingletonDeclarationKind::Output, "string", "number");
    assert_completion_excludes_labels!(&completion_suggestions, BuiltinFunctionName);
    assert!(completion_suggestions
        .iter()
        .all(|completion_suggestion| matches!(completion_suggestion.kind, CompletionItemKind::KEYWORD)));
}

#[test]
fn suppresses_existing_singleton_keywords_in_top_level_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        <cursor>

        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        input {
            instruction: string
        }

        secrets {
            api_key: string
        }

        output {
            value: null
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Tool
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        SingletonDeclarationKind::Input,
        SingletonDeclarationKind::Secrets,
        SingletonDeclarationKind::Output,
        "string",
        "number"
    );
}

#[test]
fn suppresses_suggestions_after_singleton_declaration_keyword_header() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        input <cursor>
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_suggestions_after_named_declaration_keyword_header() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent <cursor>
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_block_braces_after_named_schema_declaration_name() {
    let completion_suggestions = inline_completion_suggestions! {
        schema test <cursor> {
        }
    };

    assert_completion_contains!(&completion_suggestions, "{}");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        ReferenceKeyword::Tool,
        "number",
        "string"
    );

    assert_eq!(completion_suggestions.len(), 1);
}

#[test]
fn suggests_only_agent_properties_in_agent_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            <cursor>
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Model,
        AgentExpressionPropertyName::Instruction,
        AgentExpressionPropertyName::Uses,
        "output"
    );

    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);
}

#[test]
fn suggests_only_agent_file_properties_in_file_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            file "content" {
                <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, AgentFilePropertyName::Name, AgentFilePropertyName::Purpose);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Instruction
    );
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);
}

#[test]
fn suggests_only_model_properties_in_model_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        model openai_model from openai {
            id: "big-pickle"
            <cursor>
            inference {
                max_tokens: 800
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "id", "inference");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        AgentExpressionPropertyName::Uses,
        "endpoint",
        "headers"
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);
}

#[test]
fn suggests_only_model_usage_properties_in_agent_model_override_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            model: model.openai_model {
                <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "inference");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "id",
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Instruction,
        AgentExpressionPropertyName::Uses,
        "endpoint",
        "headers"
    );
}

#[test]
fn suggests_model_usage_properties_with_newline_before_override_block() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            model: model.openai_model
            {
                <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "inference");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "id",
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Instruction,
        "endpoint"
    );
}

#[test]
fn suggests_only_provider_properties_in_provider_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "endpoint", "api_key");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        "id",
        "headers"
    );
}

#[test]
fn suggests_only_mcp_server_properties_in_mcp_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        mcp local {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "endpoint", "headers");

    let headers_completion = completion_suggestion_by_label(&completion_suggestions, "headers");

    assert_eq!(headers_completion.insert_text, "headers {\n    $1\n}");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        "id",
        "api_key"
    );
}

#[test]
fn suggests_mcp_server_properties_with_newline_before_block() {
    let completion_suggestions = inline_completion_suggestions! {
        mcp local
        {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "endpoint", "headers");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        "id",
        "api_key"
    );
}

#[test]
fn suggests_context_operators_and_agents_for_agent_context_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent example {
            context: <cursor>
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        ExpressionKeyword::Context,
        ExpressionKeyword::Compact,
        ReferenceKeyword::Agent,
        "example"
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        BuiltinFunctionName::Template,
        ReferenceKeyword::Tool,
        "string",
        "number"
    );

    assert_eq!(completion_suggestions.len(), 4);
}
