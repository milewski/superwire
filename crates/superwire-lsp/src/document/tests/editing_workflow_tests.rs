use super::*;

#[test]
fn completes_tool_names_while_mcp_batch_import_is_incomplete() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local.tool {
            tool upd<cursor>
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "update_user_name");
    assert_completion_excludes_labels!(&completion_suggestions, "list_all_participants_who_has_answered_given_task");

    let tool_completion = completion_suggestion_by_label(&completion_suggestions, "update_user_name");

    assert_eq!(tool_completion.insert_text, "update_user_name");
}

#[test]
fn completes_declared_tools_inside_partial_uses_array() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }
        }

        tool issue_tracker_lookup {
            input {
                issue_id: number
            }
        }

        agent tooling {
            uses: [tool.knowledge_base_search, tool.<cursor>]
            instruction: "Review the issue"
            output {
                value: string
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "issue_tracker_lookup");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Instruction,
        DeclarationKeyword::Agent
    );

    let tool_completion = completion_suggestion_by_label(&completion_suggestions, "issue_tracker_lookup");

    assert_eq!(tool_completion.insert_text, "issue_tracker_lookup");
}

#[test]
fn completes_partial_for_loop_destructuring_fields() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        agent findings {
            output {
                items: [{
                    id: string
                    name: string
                    severity: number
                }]
            }
        }

        agent remediation_plan for { id, na<cursor> } in agent.findings.items {
            instruction: "Resolve {{ id }}"
            output {
                value: string
            }
        }
    });
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, "name");
    assert_completion_excludes_labels!(&completion_suggestions, "id", "severity");

    let destructuring_completion = completion_suggestion_by_label(&completion_suggestions, "name");

    assert_eq!(destructuring_completion.insert_text, "name");
}

#[test]
fn completes_provider_driver_prefix_while_provider_is_edited() {
    let completion_suggestions = inline_completion_suggestions! {
        provider llm from op<cursor> {}
    };

    assert_completion_contains_labels!(&completion_suggestions, "openai", "openai_compatible", "openrouter");
    assert_completion_excludes_labels!(&completion_suggestions, "anthropic", DeclarationKeyword::Agent);

    let provider_driver_completion = completion_suggestion_by_label(&completion_suggestions, "openai");

    assert_eq!(provider_driver_completion.insert_text, "openai");
}

#[test]
fn replacement_range_covers_partial_model_name() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        provider openai from openai {}

        model openai_gpt_4_1_mini from openai {
            id: "gpt-4.1-mini"
        }

        model openai_gpt_4o_mini from openai {
            id: "gpt-4o-mini"
        }

        agent writer {
            model: op<cursor>
            instruction: "hello"
            output {
                value: string
            }
        }
    });
    let completion_suggestions = completion_suggestions_from_source(source.clone(), cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, "openai_gpt_4_1_mini", "openai_gpt_4o_mini");
    assert_completion_excludes_labels!(&completion_suggestions, "openai", DeclarationKeyword::Agent);

    let model_completion = completion_suggestion_by_label(&completion_suggestions, "openai_gpt_4_1_mini");

    assert_eq!(model_completion.insert_text, "model.openai_gpt_4_1_mini");

    let document_state = DocumentState::new(source, None);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("model name completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character - 2);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn reports_discriminator_conflict_while_schema_variant_case_is_edited() {
    let diagnostics = inline_diagnostics! {
        schema api_event {
            variant type {
                user_created {
                    type: string
                    user_id: string
                }

                user_deleted {
                    user_id: string
                }
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidVariantDiscriminatorField);
}
