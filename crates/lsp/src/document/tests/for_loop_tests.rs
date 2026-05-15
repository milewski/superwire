use super::*;

#[test]
fn completes_input_fields_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            products: [string]
        }

        agent worker for item in input.<cursor> {
            instruction: item
        }
    };

    assert_completion_contains!(&completion_suggestions, "products");
}

#[test]
fn completes_agent_names_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings_source {
            output {
                value: [string]
            }
        }

        agent remediation_plan for finding in agent.<cursor> {
            instruction: finding
        }
    };

    assert_completion_contains!(&completion_suggestions, "findings_source");
    assert_completion_excludes_labels!(&completion_suggestions, "remediation_plan");
}

#[test]
fn suppresses_non_iterable_input_field_suggestions_in_for_loop_iterable_reference() {
    let source_template = inline_document_template! {
        input {
            xxxx: string
        }

        agent worker for item in input.<cursor> {
            instruction: item
        }
    };

    let (source_text, cursor_position) = source_with_cursor(source_template);
    let line_text = source_text
        .lines()
        .nth(cursor_position.line as usize)
        .expect("cursor line should exist");
    let line_prefix = line_text.chars().take(cursor_position.character as usize).collect::<String>();

    assert!(
        super::super::completion_context::ForLoopIterableValueCompletionContext::from_line_prefix(&line_prefix).is_some(),
        "for-loop iterable value context should be detected for line prefix {line_prefix:?}"
    );

    assert!(
        matches!(
            super::super::reference::ReferenceCompletionConstraint::from_line_prefix(&line_prefix),
            super::super::reference::ReferenceCompletionConstraint::ForLoopIterable
        ),
        "reference completion should require iterable constraint"
    );

    let completion_suggestions = inline_completion_suggestions! {
        input {
            xxxx: string
        }

        agent worker for item in input.<cursor> {
            instruction: item
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn completes_iterable_secrets_fields_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        secrets {
            finding_ids: [string]
            api_key: string
        }

        agent remediation_plan for finding in secrets.<cursor> {
            instruction: finding
        }
    };

    assert_completion_contains!(&completion_suggestions, "finding_ids");
}

#[test]
fn suggests_agent_properties_inside_for_loop_agent_block() {
    let completion_suggestions = inline_completion_suggestions! {
        agent source {}

        agent worker for item in agent.source {
            <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, AgentExpressionPropertyName::Instruction);
    assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
}

#[test]
fn suggests_inference_settings_inside_for_loop_agent_inference_object() {
    let completion_suggestions = inline_completion_suggestions! {
        agent number_note for number in [1, 2, 3, 4] {
            model: model.fast {
                inference {
                    <cursor>
                }
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);
    assert_completion_excludes_labels!(&completion_suggestions, AgentExpressionPropertyName::Uses);
}

#[test]
fn suggests_for_loop_iterator_inside_prompt_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            numbers: [number]
        }

        agent input_number_note for n in input.numbers {
            model: model.ollama_model
            instruction: "Write a short note for input number {{ <cursor> }}"
            output {
                number: number
                note: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "n");
}

#[test]
fn completes_iterator_object_fields_from_agent_for_loop_iterable() {
    let completion_suggestions = inline_completion_suggestions! {
        agent number_note for number in [1, 2, 3, 4] {
            output {
                /// numeric message index
                number: number
                /// generated note text
                note: string
            }
        }

        agent input_number_note for n in agent.number_note {
            instruction: "Write a short note for input number {{ n.<cursor> }}"
            output {
                number: number
                note: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "number", "note");

    let number_field_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == "number")
        .expect("number field completion should exist");

    let note_field_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == "note")
        .expect("note field completion should exist");

    assert_eq!(number_field_completion.documentation, "numeric message index");
    assert_eq!(note_field_completion.documentation, "generated note text");
}

#[test]
fn suggests_iterator_name_for_agent_iterable_for_loop() {
    let completion_suggestions = inline_completion_suggestions! {
        agent number_note for number in [1, 2, 3, 4] {
            output {
                number: number
                note: string
            }
        }

        agent input_number_note for n in agent.number_note {
            instruction: "Write a short note {{ <cursor> }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "n");
}

#[test]
fn suggests_only_valid_iterable_values_after_for_in_clause() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan for something in <cursor> {
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets,
        "[]"
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "boolean",
        "number",
        ReferenceKeyword::Tool,
        BuiltinFunctionName::Context
    );
}

#[test]
fn suggests_for_keyword_after_agent_name_in_agent_header() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan <cursor> {
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::For);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Instruction
    );

    let for_keyword_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == ForClauseKeyword::For.as_str())
        .expect("for keyword completion should exist");

    assert!(matches!(for_keyword_completion.kind, CompletionKind::Keyword));
}

#[test]
fn suggests_in_keyword_after_for_iterator_name_in_agent_header() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan for item <cursor> {
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::In);
    assert_completion_excludes_labels!(&completion_suggestions, ForClauseKeyword::For, DeclarationKeyword::Agent);

    let in_keyword_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == ForClauseKeyword::In.as_str())
        .expect("in keyword completion should exist");

    assert!(matches!(in_keyword_completion.kind, CompletionKind::Keyword));
}

#[test]
fn suggests_in_keyword_after_for_object_destructuring_pattern_in_agent_header() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan for { id, name } <cursor> {
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::In);
    assert_completion_excludes_labels!(&completion_suggestions, ForClauseKeyword::For, DeclarationKeyword::Agent);
}

#[test]
fn suggests_destructuring_field_names_from_agent_iterable_output() {
    let completion_suggestions = completion_suggestions_from_source_without_cursor_normalization(inline_document_template! {
        input {
            findings_text: string
        }

        agent findings {
            model: model.ollama_model
            instruction: "Parse this text into a short list of findings: {{ input.findings_text }}"
            output {
                items: [{
                    id: string
                    name: number
                }]
            }
        }

        agent remediation_plan for { <cursor> } in agent.findings.items {
            instruction: "{{ id }}"
            output {
                value: string
            }
        }
    });

    assert_completion_contains!(&completion_suggestions, "id", "name");
}

#[test]
fn excludes_existing_destructured_field_names_from_suggestions() {
    let completion_suggestions = completion_suggestions_from_source_without_cursor_normalization(inline_document_template! {
        input {
            findings_text: string
        }

        agent findings {
            model: model.ollama_model
            instruction: "Parse this text into a short list of findings: {{ input.findings_text }}"
            output {
                items: [{
                    id: string
                    name: number
                }]
            }
        }

        agent remediation_plan for { id, <cursor> } in agent.findings.items {
            instruction: "{{ id }}"
            output {
                value: string
            }
        }
    });

    assert_completion_contains!(&completion_suggestions, "name");
    assert_completion_excludes_labels!(&completion_suggestions, "id");
}

fn completion_suggestions_from_source_without_cursor_normalization(source_template: &str) -> Vec<CompletionSuggestion> {
    let cursor_marker = "<cursor>";
    let cursor_byte_offset = source_template
        .find(cursor_marker)
        .expect("cursor marker should exist in test source");
    let mut line = 0_u32;
    let mut character = 0_u32;

    for source_character in source_template[..cursor_byte_offset].chars() {
        if source_character == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    let source_without_cursor = source_template.replacen(cursor_marker, "", 1);

    completion_suggestions_from_source(source_without_cursor, Position { line, character })
}

#[test]
fn suggests_object_destructuring_bindings_inside_prompt_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        agent alpha {
            output {
                participants: [{
                    id: number
                    name: string
                    profile: {
                        city: string
                    }
                }]
            }
        }

        agent analyzer for { id, profile } in agent.alpha.participants {
            instruction: "Analyze participant {{ <cursor> }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "id", "profile");
    assert_completion_excludes_labels!(&completion_suggestions, "name");
}

#[test]
fn completes_object_destructuring_binding_fields_from_for_loop_iterable() {
    let completion_suggestions = inline_completion_suggestions! {
        agent alpha {
            output {
                participants: [{
                    id: number
                    name: string
                    profile: {
                        city: string
                    }
                }]
            }
        }

        agent analyzer for { id, profile } in agent.alpha.participants {
            instruction: "Analyze participant {{ profile.<cursor> }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "city");
    assert_completion_excludes_labels!(&completion_suggestions, "id", "name");
}

#[test]
fn completes_iterable_references_for_object_destructuring_for_clause() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            participants: [{
                id: number
                name: string
            }]
        }

        agent analyzer for { id, name } in input.<cursor> {
            instruction: "Analyze participant {{ id }} {{ name }}"
            output {
                value: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "participants");
}
