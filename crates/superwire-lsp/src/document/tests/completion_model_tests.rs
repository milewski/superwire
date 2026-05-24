use super::*;

#[test]
fn suggests_only_inference_settings_inside_inference_object() {
    let completion_suggestions = inline_completion_suggestions! {
        model fast from openai {
            inference {
                <cursor>
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Model,
        DeclarationKeyword::Provider
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);
}

#[test]
fn suggests_context_operators_and_agents_inside_context_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent research {
            instruction: "research"
        }

        agent summarize {
            context: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "context", "compact", "agent", "research", "summarize");
}

#[test]
fn suggests_inference_settings_with_newline_before_inference_block() {
    let completion_suggestions = inline_completion_suggestions! {
        model fast from openai {
            inference
            {
                <cursor>
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Model,
        DeclarationKeyword::Provider,
        "id"
    );
}

#[test]
fn suggests_inference_settings_inside_model_usage_override() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            model: model.fast {
                inference {
                    <cursor>
                }
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);

    assert_completion_excludes_labels!(&completion_suggestions, AgentExpressionPropertyName::Model, "number", "string");
}

#[test]
fn suppresses_inference_suggestions_inside_string_literal_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            model: model.fast {
                inference {
                    max_tokens: "<cursor>"
                }
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_inference_value_reference_roots_for_integer_setting() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            max_tokens: number
            label: string
        }

        schema limits {
            max_tokens: number
        }

        agent helper {
            instruction: "hello"
            output {
                max_tokens: number
                label: string
            }
        }

        agent writer {
            model: model.fast {
                inference {
                    max_tokens: <cursor>
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Agent, ReferenceKeyword::Input);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Provider,
        "string",
        "number"
    );
}

#[test]
fn suggests_only_numeric_resolving_input_fields_for_integer_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            max_tokens: number
            metadata: {
                nested_limit: number
            }
            label: string
        }

        agent writer {
            model: model.fast {
                inference {
                    max_tokens: input.<cursor>
                }
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "max_tokens", "metadata");
    assert_completion_excludes_labels!(&completion_suggestions, "label");
}

#[test]
fn suggests_numeric_input_fields_for_model_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            penalty: number
            metadata: {
                nested_penalty: number
            }
            label: string
        }

        model openai_model from openai {
            id: "model-a"
            inference {
                frequency_penalty: input.<cursor>
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "penalty", "metadata");
    assert_completion_excludes_labels!(&completion_suggestions, "label");
}

#[test]
fn suggests_numeric_agent_outputs_for_model_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent scorer {
            instruction: "score"
            output {
                value: number
            }
        }

        agent labeler {
            instruction: "label"
            output {
                value: string
            }
        }

        model openai_model from openai {
            id: "model-a"
            inference {
                frequency_penalty: agent.<cursor>
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "scorer");
    assert_completion_excludes_labels!(&completion_suggestions, "labeler");
}

#[test]
fn suppresses_schema_reference_suggestions_for_integer_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        schema limits {
            max_tokens: number
        }

        agent writer {
            model: model.fast {
                inference {
                    max_tokens: schema.<cursor>
                }
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_agent_properties_before_inference_block() {
    let completion_suggestions = inline_completion_suggestions! {
        agent release_analyst {
            model: model.openai_model

            <cursor>

            model: model.openai_model {
                inference {
                    temperature: 0.2
                    max_tokens: 12_000
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, AgentExpressionPropertyName::Instruction);
    assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
}

#[test]
fn includes_descriptive_details_for_agent_and_inference_completions() {
    let agent_completions = inline_completion_suggestions! {
        agent writer {
            <cursor>
        }
    };

    let inference_completions = inline_completion_suggestions! {
        model fast from openai {
            inference {
                <cursor>
            }
        }
    };

    let model_completion = agent_completions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == "model")
        .expect("agent completion should include model property");

    let max_tokens_completion = inference_completions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == InferenceSetting::MaxTokens.key())
        .expect("inference completion should include max_tokens setting");

    assert_eq!(model_completion.detail, "Model binding (required)");
    assert_eq!(max_tokens_completion.detail, "Token budget (integer)");
}

#[test]
fn completes_registered_provider_models_inside_model_call() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_gpt_4_1_mini from openai {
            id: "gpt-4.1-mini"
        }

        model openai_gpt_4o_mini from openai {
            id: "gpt-4o-mini"
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "openai_gpt_4_1_mini", "openai_gpt_4o_mini");

    let gpt_model_completion = completion_suggestion_by_label(&completion_suggestions, "openai_gpt_4_1_mini");

    assert_eq!(gpt_model_completion.insert_text, "model.openai_gpt_4_1_mini");
    assert_eq!(gpt_model_completion.detail, "gpt-4.1-mini");
}

#[test]
fn completes_dynamic_provider_models_inside_empty_string_model_call() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: secrets.models.pro
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    let dynamic_model_completion = completion_suggestion_by_label(&completion_suggestions, "openai_model");

    assert_eq!(dynamic_model_completion.insert_text, "model.openai_model");
}

#[test]
fn suggests_only_declared_providers_for_model_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        provider anthropic from anthropic {}

        model anthropic_model from anthropic {
            id: "claude-3-7-sonnet-latest"
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "openai_model", "anthropic_model");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "context",
        ReferenceKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        DeclarationKeyword::Provider,
        InferenceSetting::MaxTokens,
        "string"
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::KEYWORD);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::MODULE);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::PROPERTY);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::VARIABLE);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::STRUCT);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);

    let openai_completion = completion_suggestion_by_label(&completion_suggestions, "openai_model");

    assert_eq!(openai_completion.insert_text, "model.openai_model");
    assert_eq!(openai_completion.detail, "gpt-4.1-mini");
}

#[test]
fn suggests_declared_models_inside_model_reference_namespace() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        model backup_model from openai {
            id: "gpt-4o-mini"
        }

        agent customer_email {
            model: model.<cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "openai_model", "backup_model");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "context",
        ReferenceKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        DeclarationKeyword::Provider,
        InferenceSetting::MaxTokens,
        "string"
    );

    let openai_completion = completion_suggestion_by_label(&completion_suggestions, "openai_model");

    assert_eq!(openai_completion.insert_text, "openai_model");
    assert_eq!(openai_completion.detail, "gpt-4.1-mini");
}

#[test]
fn suggests_declared_models_inside_for_loop_agent_model_reference_namespace() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            data: {
                participants: [{ id: number }]
            }
        }

        provider openai from openai {}

        model participant_answer_model from openai {
            id: "gpt-4.1-mini"
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: model.<cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "participant_answer_model");

    let model_completion = completion_suggestion_by_label(&completion_suggestions, "participant_answer_model");

    assert_eq!(model_completion.insert_text, "participant_answer_model");
}

#[test]
fn suggests_secrets_fields_inside_model_id_reference_namespace() {
    let completion_suggestions = inline_completion_suggestions! {
        secrets {
            models: {
                pro: string
            }
            api_key: string
        }

        model max from openai {
            id: secrets.<cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "models", "api_key");
}

#[test]
fn suggests_only_declared_providers_for_model_declaration_provider() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        provider openai from openai {
            endpoint: "http://100.118.249.48:3000/v1"
            api_key: "xxxx"
        }

        model openai_model from <cursor> {
            id: "big-pickle"
        }
    });
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, "openai");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "context",
        ReferenceKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        DeclarationKeyword::Provider,
        InferenceSetting::MaxTokens,
        "string"
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::KEYWORD);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::MODULE);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::PROPERTY);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::VARIABLE);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::STRUCT);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionItemKind::FUNCTION);

    let openai_completion = completion_suggestion_by_label(&completion_suggestions, "openai");

    assert_eq!(openai_completion.insert_text, "openai");
}

#[test]
fn suppresses_provider_drivers_for_model_declaration_provider_without_declared_providers() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        model pro from <cursor> {
            id: "model-a"
        }
    });

    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "anthropic",
        "google",
        "openai",
        "openai_compatible",
        "anthropic_compatible",
        "ollama"
    );
}

#[test]
fn suggests_only_declared_providers_for_model_declaration_provider_with_driver_named_provider() {
    let completion_suggestions = inline_completion_suggestions! {
        provider my_model from openai_compatible {
            endpoint: "http://100.118.249.48:3000/v1"
        }

        model openai_model from <cursor> {
            id: "mimo-v2.5"
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "my_model");
    assert_completion_excludes_labels!(&completion_suggestions, "openai_compatible", "openai", "anthropic");
}

#[test]
fn suggests_provider_drivers_for_provider_declaration_driver() {
    let completion_suggestions = inline_completion_suggestions! {
        provider llm from <cursor> {}
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        "anthropic",
        "google",
        "openai",
        "openai_compatible",
        "anthropic_compatible",
        "ollama"
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        ReferenceKeyword::Agent,
        AgentExpressionPropertyName::Instruction,
        "context",
        "string"
    );
}

#[test]
fn filters_provider_drivers_for_provider_declaration_driver_prefix() {
    let completion_suggestions = inline_completion_suggestions! {
        provider llm from op<cursor> {}
    };

    assert_completion_contains_labels!(&completion_suggestions, "openai", "openai_compatible", "openrouter");
    assert_completion_excludes_labels!(&completion_suggestions, "anthropic", "google", "ollama");
}

#[test]
fn suggests_reference_roots_inside_model_call_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai from openai {}

        model openai_gpt_4_1_mini from openai {
            id: "gpt-4.1-mini"
        }

        model openai_gpt_4o_mini from openai {
            id: "gpt-4o-mini"
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "openai_gpt_4_1_mini", "openai_gpt_4o_mini");

    let gpt_model_completion = completion_suggestion_by_label(&completion_suggestions, "openai_gpt_4_1_mini");

    assert_eq!(gpt_model_completion.insert_text, "model.openai_gpt_4_1_mini");
}

#[test]
fn completion_text_edit_range_replaces_model_provider_prefix() {
    let source = inline_document_template! {
        provider openai from openai {}

        model openai_gpt_4_1_mini from openai {
            id: "gpt-4.1-mini"
        }

        model openai_gpt_4o_mini from openai {
            id: "gpt-4o-mini"
        }

        agent writer {
            model: op
            instruction: "hello"
            output {
                value: string
            }
        }
    }
    .to_string();

    let model_line_index = source
        .lines()
        .position(|source_line| source_line.contains("model: op"))
        .expect("model line should exist in source");

    let model_line = source
        .lines()
        .nth(model_line_index)
        .expect("model line should exist by discovered line index");

    let provider_prefix_character_index = model_line
        .find("op")
        .map(|provider_prefix_start| provider_prefix_start + "op".chars().count())
        .expect("model line should contain provider prefix");

    let cursor_position = Position {
        line: u32::try_from(model_line_index).expect("model line index should fit in u32"),
        character: u32::try_from(provider_prefix_character_index).expect("provider prefix character index should fit in u32"),
    };

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("model property completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character - 2);
}

#[test]
fn completion_text_edit_range_inserts_model_name_at_empty_string_cursor() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        provider openai from openai {}

        model openai_gpt_4_1_mini from openai {
            id: "gpt-4.1-mini"
        }

        model openai_gpt_4o_mini from openai {
            id: "gpt-4o-mini"
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("model name completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_replaces_empty_model_string_for_dynamic_model() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        provider openai from openai {}

        model openai_model from openai {
            id: secrets.models.pro
        }

        agent writer {
            model: <cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("dynamic model completion should replace the string literal");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}
