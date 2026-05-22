use super::*;

#[test]
fn uses_mcp_lock_for_imported_tool_schema() {
    let source = inline_document_template! {
        mcp local {
            endpoint: "http://docker.localhost/mcp/project"
        }

        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        tool update_user_name from mcp.local.tool.update_user_name

        agent tooling {
            model: model.openai_model
            uses: [tool.update_user_name]
            instruction: "Rename the user"
            output {
                value: string
            }
        }
    };

    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));

    assert!(document_state.diagnostics().is_empty());
}

#[test]
fn accepts_local_output_schema_on_imported_mcp_tool() {
    let source = inline_document_template! {
        mcp local {
            endpoint: "http://docker.localhost/mcp/project"
        }

        tool fetch_numbers from mcp.local.tool.fetch_numbers {
            output {
                values: string
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));

    let diagnostics = document_state.diagnostics();

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:#?}");
}

#[test]
fn accepts_manual_output_schema_that_filters_mcp_output_schema() {
    let source = inline_document_template! {
        mcp local {
            endpoint: "http://docker.localhost/mcp/project"
        }

        tool fetch_participant_answer from mcp.local.tool.fetch_participant_answer {
            output {
                answer: variant task_type {
                    open_written {
                        text: string
                    }
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));

    let diagnostics = document_state.diagnostics();

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:#?}");
}

#[allow(clippy::too_many_lines)]
#[test]
fn suggests_mcp_tool_names_inside_tool_import_path() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        tool imported_tool from mcp.local.tool.<cursor>
    });

    assert_completion_contains_labels!(
        &completion_suggestions,
        "list_all_participants_who_has_answered_given_task",
        "update_user_name"
    );
}

#[test]
fn suggests_mcp_servers_after_mcp_namespace_root() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        mcp.<cursor> {
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "local");
}

#[test]
fn suggests_mcp_servers_inside_batch_import_header() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.<cursor> {
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "local");
}

#[test]
fn suggests_declared_mcp_server_inside_incomplete_batch_import_without_lock() {
    let completion_suggestions = inline_completion_suggestions! {
        mcp mintilify {
            endpoint: "https://acme-796e8c63.mintlify.app/mcp"
        }

        from mcp.<cursor> {
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "mintilify");
    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Agent, DeclarationKeyword::Provider);
}

#[test]
fn excludes_recovery_placeholder_inside_incomplete_mcp_batch_import() {
    let completion_suggestions = inline_completion_suggestions! {
        mcp mintilify {
            endpoint: "https://acme-796e8c63.mintlify.app/mcp"
        }

        from mcp.mintilify {
            <cursor>
        }
    };

    assert_completion_excludes_labels!(&completion_suggestions, "__completion_placeholder");
    assert!(
        completion_suggestions
            .iter()
            .all(|completion_suggestion| !completion_suggestion.insert_text.contains("__completion_placeholder")),
        "recovery placeholder should not appear in completion insert text: {completion_suggestions:?}"
    );
}

#[test]
fn suggests_mcp_import_namespaces_after_server() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        mcp.local.<cursor> {
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "tool", "resource", "prompt");
}

#[test]
fn suggests_mcp_tool_names_inside_batch_tool_import_item() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local.tool {
            bindings {
                project_id: 1
            }

            tool <cursor>
        }
    });

    assert_completion_contains_labels!(
        &completion_suggestions,
        "list_all_participants_who_has_answered_given_task",
        "update_user_name"
    );
}

#[test]
fn excludes_already_imported_mcp_tool_names_inside_batch_tool_import_item() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task
            tool <cursor>
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "update_user_name");
    assert_completion_excludes_labels!(&completion_suggestions, "list_all_participants_who_has_answered_given_task");
}

#[test]
fn suggests_only_tool_keyword_inside_scoped_tool_batch_import() {
    let completion_suggestions = inline_completion_suggestions! {
        from mcp.local.tool {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "tool");
    assert_completion_excludes_labels!(&completion_suggestions, "prompt", "resource", DeclarationKeyword::Agent);
}

#[test]
fn suggests_item_keywords_inside_unscoped_batch_import() {
    let completion_suggestions = inline_completion_suggestions! {
        from mcp.local {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "tool", "prompt", "resource");
    assert_completion_excludes_labels!(&completion_suggestions, "input", "bindings", "max_calls", "output");
}

#[test]
fn suggests_mcp_prompt_names_inside_unscoped_batch_prompt_item() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local {
            prompt <cursor>
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "system_prompt", "review_prompt");
}

#[test]
fn excludes_already_imported_prompt_names_inside_unscoped_batch_prompt_item() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local {
            prompt system_prompt
            prompt <cursor>
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "review_prompt");
    assert_completion_excludes_labels!(&completion_suggestions, "system_prompt");
}

#[test]
fn suggests_mcp_output_fields_inside_imported_tool_output_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }

            tool list_all_participants_who_has_answered_given_task {
                output {
                    <cursor>
                }
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "participants");

    assert_eq!(completion_suggestion.insert_text, "participants: [{}]");
}

#[test]
fn suggests_common_mcp_input_fields_inside_batch_input_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            input {
                <cursor>
            }

            tool list_all_participants_who_has_answered_given_task
            tool update_user_name
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "common_shared_among_all_feature");
    assert_completion_excludes_labels!(&completion_suggestions, "project_id", "task_id", "user_name");
}

#[test]
fn suggests_common_mcp_output_fields_inside_batch_output_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            output {
                <cursor>
            }

            tool list_all_participants_who_has_answered_given_task
            tool update_user_name
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "shared");
    assert_completion_excludes_labels!(&completion_suggestions, "participants", "success");
}

#[test]
fn suggests_mcp_input_fields_inside_tool_binding_override_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                bindings {
                    <cursor>
                }
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "project_id");

    assert_eq!(completion_suggestion.insert_text, "project_id: $1");
}

#[test]
fn inserts_mcp_bindings_as_value_assignments_with_doc_comments() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                bind<cursor>
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "bindings");

    assert!(completion_suggestion.insert_text.contains("/// Project identifier"));
    assert!(completion_suggestion.insert_text.contains("project_id: $1"));
    assert!(!completion_suggestion.insert_text.contains("project_id: string"));
}

#[test]
fn does_not_suggest_mcp_input_fields_at_root_of_batch_tool_body() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }

            tool list_all_participants_who_has_answered_given_task {
                <cursor>
            }
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "input", "output");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        "common_shared_among_all_feature",
        "project_id",
        "task_id",
        "participants",
        "shared",
    );
}

#[test]
fn inserts_mcp_output_schema_when_completing_output_property() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                out<cursor>
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "output");

    assert_eq!(
        completion_suggestion.insert_text,
        "output {\n    participants: [{}]\n    shared: string\n}"
    );
}

#[test]
fn inserts_expanded_mcp_output_schema_with_contextual_indentation() {
    let completion_suggestions = completion_suggestions_with_mcp_lock_without_cursor_normalization(inline_document_template! {
        from mcp.local {
            tool get_task_group_tasks {
                out<cursor>
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "output");

    assert_eq!(
        completion_suggestion.insert_text,
        "output {\n    task_group_id: number\n    task_group_title: string\n    tasks: [\n        {\n            description: string,\n            duration: number,\n            id: number,\n            mandatory: boolean,\n            options: string,\n            title: string,\n            type: string,\n        }\n    ]\n}"
    );
}

#[test]
fn inserts_mcp_output_schema_with_nullable_fields_using_maybe_syntax() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            tool fetch_participant_answer {
                out<cursor>
            }
        }
    });

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "output");

    assert!(completion_suggestion.insert_text.contains("/// Answer"));
    assert!(completion_suggestion.insert_text.contains("/// The text content of the answer"));
    assert!(completion_suggestion.insert_text.contains("/// The ID of the participant"));
    assert!(completion_suggestion.insert_text.contains("/// The ID of the task"));
    assert!(completion_suggestion.insert_text.contains("text: maybe string"));
    assert!(!completion_suggestion.insert_text.contains("| null"));
}

#[test]
fn accepts_structurally_matching_mcp_output_schema_from_lock_file() {
    let source = inline_document_template! {
        from mcp.local {
            tool get_task_group_tasks {
                output {
                    task_group_id: number
                    task_group_title: string
                    tasks: [{ description: string, duration: number, id: number, mandatory: boolean, options: string, title: string, type: string }]
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidToolBinding));
}

#[test]
fn offers_code_action_to_fill_mcp_output_schema_block() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        from mcp.local.tool {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }

            tool list_all_participants_who_has_answered_given_task {
                output {
                    <cursor>
                }
            }
        }
    });
    let document_state = DocumentState::new(source, Some(test_mcp_lock()));
    let code_actions = document_state.code_actions(cursor_position);

    assert_eq!(code_actions.len(), 1);
    assert_eq!(code_actions[0].title, "Fill output schema from MCP lock");
    assert!(code_actions[0].edit.new_text.contains("participants: [{}]"));
}

#[test]
fn indexes_batch_imported_tools_for_agent_references() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        from mcp.local.tool {
            tool update_user_name
        }

        agent tooling {
            uses: [tool.<cursor>]
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "update_user_name");
}

#[test]
fn suggests_mcp_resource_names_inside_resource_import_path() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        resource imported_resource from mcp.local.resource.<cursor>
    });

    assert_completion_contains_labels!(&completion_suggestions, "project_readme", "release_notes");
}

#[test]
fn suggests_mcp_prompt_names_inside_prompt_import_path() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        prompt imported_prompt from mcp.local.prompt.<cursor>
    });

    assert_completion_contains_labels!(&completion_suggestions, "system_prompt", "review_prompt");
}

#[test]
fn suggests_only_bindings_inside_prompt_import_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        prompt from mcp.local.prompt.dynamic_summary_prompt {
            <cursor>
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "bindings");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        "description",
        "input",
        "output",
        "max_calls",
        "tool",
        "resource",
        "prompt"
    );
}

#[test]
fn suggests_mcp_prompt_arguments_inside_prompt_import_bindings_block() {
    let completion_suggestions = completion_suggestions_with_mcp_lock(inline_document_template! {
        prompt from mcp.local.prompt.dynamic_summary_prompt {
            bindings {
                <cursor>
            }
        }
    });

    assert_completion_contains_labels!(&completion_suggestions, "project_id", "user_id");

    let project_id_completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "project_id");
    assert_eq!(project_id_completion_suggestion.insert_text, "project_id: $1");
    assert_eq!(project_id_completion_suggestion.detail, "Required prompt argument");
    assert_eq!(project_id_completion_suggestion.documentation, "Project identifier to summarize");

    let user_id_completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "user_id");
    assert_eq!(user_id_completion_suggestion.insert_text, "user_id: $1");
    assert_eq!(user_id_completion_suggestion.detail, "Optional prompt argument");
    assert_eq!(
        user_id_completion_suggestion.documentation,
        "Optional user context for personalization"
    );
}
