use super::*;

#[test]
fn suggests_only_valid_output_value_roots_and_literals_in_output_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            value: <cursor>
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets
    );
    assert_completion_contains!(&completion_suggestions, "{}", "[]", "\"\"", "0", "true", "false", "null");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        ReferenceKeyword::Tool
    );

    let agent_root_completion = completion_suggestion_by_label(&completion_suggestions, ReferenceKeyword::Agent.as_str());
    assert_eq!(agent_root_completion.insert_text, ReferenceKeyword::Agent.as_str());
}

#[test]
fn suggests_agent_names_after_output_agent_root_separator() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeter {
            output {
                value: string
            }
        }

        output {
            greeting: agent.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "greeter");
}

#[test]
fn suppresses_invalid_reference_roots_in_output_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            value: schema.<cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_valid_output_values_for_root_output_field() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            manual_numbers: <cursor>
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets
    );
    assert_completion_contains!(&completion_suggestions, "{}", "[]", "\"\"", "0");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        "number",
        "string",
        ReferenceKeyword::Tool,
        DeclarationKeyword::Provider
    );
}

#[test]
fn suggests_only_valid_prompt_value_roots_and_literals() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            instruction: <cursor>
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        "\"\"",
        "\"\"\""
    );
    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        ReferenceKeyword::Tool,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        "number",
        "string"
    );

    let single_line_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"");
    let multiline_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"\"");

    assert_eq!(single_line_string_completion.insert_text, "\"\"");
    assert_eq!(multiline_string_completion.insert_text, "\"\"\"\n\"\"\"");
}

#[test]
fn uses_current_line_indentation_for_multiline_prompt_literal_completion() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            instruction: <cursor>
            output {
                value: string
            }
        }
    });

    let prompt_line = source
        .lines()
        .nth(usize::try_from(cursor_position.line).expect("cursor line should fit usize"))
        .expect("prompt line should exist");

    let prompt_line_indentation = prompt_line
        .char_indices()
        .find_map(|(character_offset, character)| (!character.is_whitespace()).then_some(character_offset))
        .map(|first_non_whitespace_offset| &prompt_line[..first_non_whitespace_offset])
        .unwrap_or_default();

    let expected_multiline_insert_text = format!("\"\"\"\n{prompt_line_indentation}\"\"\"");

    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);
    let multiline_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"\"");

    assert_eq!(multiline_string_completion.insert_text, expected_multiline_insert_text);
}

#[test]
fn suppresses_invalid_prompt_reference_roots() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            instruction: secrets.<cursor>
            output {
                value: string
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_dynamic_agent_property_keyword() {
    let agent_property_completion_suggestions = inline_completion_suggestions! {
        agent writer {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&agent_property_completion_suggestions, "dynamic");
}

#[test]
fn suggests_only_global_dynamic_fields_outside_agents() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            global_topic: "release"
            global_limit: 5
        }

        agent alpha {
            dynamic {
                alpha_only: "alpha"
            }

            instruction: "hello"
            output {
                value: string
            }
        }

        agent beta {
            dynamic {
                beta_only: "beta"
            }

            instruction: "hello"
            output {
                value: string
            }
        }

        output {
            value: dynamic.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "global_topic", "global_limit");
    assert_completion_excludes_labels!(&completion_suggestions, "alpha_only", "beta_only");
}

#[test]
fn suggests_global_and_local_dynamic_fields_inside_agent() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            global_topic: "release"
            global_limit: 5
        }

        agent alpha {
            dynamic {
                alpha_only: "alpha"
            }

            instruction: dynamic.<cursor>
            output {
                value: string
            }
        }

        agent beta {
            dynamic {
                beta_only: "beta"
            }

            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "global_topic", "global_limit", "alpha_only");
    assert_completion_excludes_labels!(&completion_suggestions, "beta_only");
}

#[test]
fn suggests_value_producing_expressions_for_dynamic_field_values() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            topic: string
        }

        secrets {
            api_key: string
        }

        agent writer {
            instruction: "hello"
            output {
                value: string
            }
        }

        dynamic {
            rendered_prompt: <cursor>
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Dynamic,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets,
        BuiltinFunctionName::Compact,
        BuiltinFunctionName::Template
    );
}

#[test]
fn filters_dynamic_value_roots_by_prefix() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            rendered_prompt: str.<cursor>
        }
    };

    assert!(
        completion_suggestions.is_empty(),
        "unexpected suggestions: {completion_suggestions:?}"
    );
}

#[test]
fn suggests_other_dynamic_fields_inside_dynamic_value() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            previous_value: "ready"
            current_value: dynamic.<cursor>
            future_value: "later"
        }
    };

    assert_completion_contains!(&completion_suggestions, "previous_value", "future_value");
    assert_completion_excludes_labels!(&completion_suggestions, "current_value");
}

#[test]
fn suggests_tools_inside_dynamic_tool_call_callee() {
    let completion_suggestions = inline_completion_suggestions! {
        tool searchable_web {
            input {
                query: string
            }

            output {
                title: string
            }
        }

        dynamic {
            search_result: call tool.<cursor> {}
        }
    };

    assert_completion_contains!(&completion_suggestions, "searchable_web");
}

#[test]
fn suggests_resources_inside_dynamic_read_callee() {
    let completion_suggestions = inline_completion_suggestions! {
        resource project_readme from mcp.local.resource.project_readme

        dynamic {
            readme: read resource.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "project_readme");
}

#[test]
fn suggests_prompts_inside_dynamic_render_callee() {
    let completion_suggestions = inline_completion_suggestions! {
        prompt system_prompt from mcp.local.prompt.system_prompt

        dynamic {
            instructions: render prompt.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "system_prompt");
}

#[test]
fn suggests_mcp_calls_for_agent_prompt_values() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            instruction: <cursor>
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, McpCallOperation::Read);
    assert_completion_contains!(&completion_suggestions, McpCallOperation::Render);
}

#[test]
fn suppresses_suggestions_before_dynamic_field_key() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            <cursor>
        }
    };

    assert!(
        completion_suggestions.is_empty(),
        "unexpected suggestions: {completion_suggestions:?}"
    );
}

#[test]
fn suggests_dynamic_fields_from_later_blocks() {
    let completion_suggestions = inline_completion_suggestions! {
        dynamic {
            a: dynamic.<cursor>
        }

        dynamic {
            max_results: 5
            timeout_seconds: 30
        }
    };

    assert_completion_contains!(&completion_suggestions, "max_results", "timeout_seconds");
    assert_completion_excludes_labels!(&completion_suggestions, "a");
}

#[test]
fn completion_text_edit_range_for_prompt_value_keeps_space_after_separator() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            instruction: <cursor>
            output {
                value: string
            }
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("prompt completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_prompt_reference_after_separator_keeps_root_and_separator() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            instruction: agent.<cursor>
            output {
                value: string
            }
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("prompt reference completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_output_reference_after_separator_keeps_root_and_separator() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent greeter {
            output {
                value: string
            }
        }

        output {
            greeting: agent.<cursor>
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("output reference completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_array_item_type_does_not_replace_opening_bracket() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            output {
                values: [<cursor>]
            }
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("array item type completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_agent_property_inserts_at_current_line_cursor() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent greeting {
            model: model.ollama_model
            <cursor>
        }
    });

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("agent property completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn suppresses_fallback_suggestions_after_terminal_agent_output_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            instruction: "hello"
            output {
                value: string
            }
        }

        output {
            greeting: agent.greeting.value.<cursor>
        }
    };

    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Instruction,
        DeclarationKeyword::Provider
    );
}

#[test]
fn suggests_agent_output_fields_for_nested_agent_output_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            instruction: "hello"
            output {
                message: string
                language: string
            }
        }

        output {
            greeting: agent.greeting.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "message", "language");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        BuiltinFunctionName::Context,
        "string",
        "number"
    );
}

#[test]
fn suppresses_field_completion_after_dot_access_on_nullable_reference_path() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            output {
                nested: maybe {
                    value: string
                }
            }
        }

        output {
            greeting: agent.greeting.nested.<cursor>
        }
    };

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Instruction
    );
}

#[test]
fn suggests_field_completion_after_optional_access_on_nullable_reference_path() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            output {
                nested: maybe {
                    value: string
                }
            }
        }

        output {
            greeting: agent.greeting.nested?.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "value");
}
