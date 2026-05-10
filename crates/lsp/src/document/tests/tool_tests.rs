use super::*;
use std::collections::BTreeMap;
use superwire_core::mcp::{McpLock, McpServerLock, McpToolLock};

#[test]
fn suggests_tool_keyword_inside_uses_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        agent tooling {
            uses: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Tool);
}

#[test]
fn suggests_declared_tools_for_tool_namespace_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }
        }

        agent tooling {
            uses: [tool.<cursor>]
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "knowledge_base_search");
}

#[test]
fn suggests_declared_tools_for_multiline_tool_namespace_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            input {
                issue_id: number
            }
        }

        agent tooling {
            uses: [
                tool.<cursor>,
            ]
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "issue_tracker_lookup");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        "context",
        "inference",
        "model",
        "output",
        "prompt",
        "tools",
    );

    assert!(
        completion_suggestions
            .iter()
            .all(|completion_suggestion| completion_suggestion.label == "issue_tracker_lookup"),
        "expected only declared tool suggestions; got {completion_suggestions:?}"
    );
}

#[test]
fn inserts_plain_tool_name_for_tool_without_bounded_arguments() {
    let completion_suggestions = inline_completion_suggestions! {
        tool web_search {
            input {
                query: string
            }
        }

        agent tooling {
            uses: [tool.<cursor>]
        }
    };

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "web_search");

    assert_eq!(completion_suggestion.insert_text, "web_search");
}

#[test]
fn inserts_binding_block_for_tool_with_bounded_arguments_when_block_does_not_exist() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            bindings {
                password: string
            }
        }

        agent tooling {
            uses: [tool.<cursor>]
        }
    };

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "issue_tracker_lookup");

    assert_eq!(
        completion_suggestion.insert_text,
        "issue_tracker_lookup {\n    bindings {\n        $1\n    }\n}"
    );
}

#[test]
fn inserts_plain_tool_name_when_binding_block_already_exists() {
    let completion_suggestions = inline_completion_suggestions! {
        secrets {
            knowledge_base_password: string
        }

        tool issue_tracker_lookup {
            bindings {
                password: string
            }
        }

        agent tooling {
            uses: [tool.<cursor> {
                bindings {
                    password: secrets.knowledge_base_password
                }
            }]
        }
    };

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "issue_tracker_lookup");

    assert_eq!(completion_suggestion.insert_text, "issue_tracker_lookup");
}

#[test]
fn suggests_only_tool_properties_inside_tool_block() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "description", "max_calls", "input", "bindings", "output");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Tool,
        DeclarationKeyword::Agent,
        "string",
        "number",
    );
}

#[test]
fn uses_mcp_lock_for_imported_tool_schema() {
    let source = inline_document_template! {
        mcp local {
            endpoint: "http://docker.localhost/mcp/project"
        }

        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        tool update_user_name from mcp.local.tool.update_user_name

        agent tooling {
            model: openai("gpt-4.1-mini")
            uses: [tool.update_user_name]
            instruction: "Rename the user"
            output: string
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

    assert!(document_state.diagnostics().is_empty());
}

fn test_mcp_lock() -> McpLock {
    let mut tools = BTreeMap::new();
    tools.insert(
        "list_all_participants_who_has_answered_given_task".to_string(),
        McpToolLock::from_json_schema_values(
            "list_all_participants_who_has_answered_given_task".to_string(),
            Some("List all participants who answered a task".to_string()),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "common_shared_among_all_feature": { "type": "string" },
                    "project_id": { "type": "number" },
                    "task_id": { "type": "number" }
                },
                "required": ["common_shared_among_all_feature", "project_id", "task_id"]
            }),
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "shared": { "type": "string" },
                    "participants": { "type": "array", "items": { "type": "object" } }
                },
                "required": ["shared", "participants"]
            })),
        )
        .expect("test MCP input schema should parse"),
    );
    tools.insert(
        "update-user-name".to_string(),
        McpToolLock::from_json_schema_values(
            "update-user-name".to_string(),
            Some("Update a user name".to_string()),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "common_shared_among_all_feature": { "type": "string" },
                    "user_name": { "type": "string" }
                },
                "required": ["common_shared_among_all_feature", "user_name"]
            }),
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "shared": { "type": "string" },
                    "success": { "type": "boolean" }
                },
                "required": ["shared", "success"]
            })),
        )
        .expect("test MCP input schema should parse"),
    );
    let mut servers = BTreeMap::new();
    servers.insert(
        "local".to_string(),
        McpServerLock {
            tools,
            resources: vec!["project-readme".to_string(), "release-notes".to_string()],
            prompts: vec!["system-prompt".to_string(), "review-prompt".to_string()],
        },
    );

    McpLock { servers }
}

fn completion_suggestions_with_mcp_lock(source_template: &str) -> Vec<CompletionSuggestion> {
    let (source, cursor_position) = source_with_cursor(source_template);
    let document_state = DocumentState::new(source, Some(test_mcp_lock()));

    document_state.completion_suggestions(cursor_position)
}

fn completion_suggestions_with_mcp_lock_without_cursor_normalization(source_template: &str) -> Vec<CompletionSuggestion> {
    let compact_cursor_marker = "<cursor>";
    let spaced_cursor_marker = "< cursor >";
    let (cursor_marker, cursor_byte_offset) = if let Some(cursor_byte_offset) = source_template.find(compact_cursor_marker) {
        (compact_cursor_marker, cursor_byte_offset)
    } else if let Some(cursor_byte_offset) = source_template.find(spaced_cursor_marker) {
        (spaced_cursor_marker, cursor_byte_offset)
    } else {
        panic!("cursor marker should exist in test source");
    };
    let mut line = 0_u32;
    let mut character = 0_u32;

    for character_in_source in source_template[..cursor_byte_offset].chars() {
        if character_in_source == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    let source_without_cursor = source_template.replacen(cursor_marker, "", 1);
    let document_state = DocumentState::new(source_without_cursor, Some(test_mcp_lock()));

    document_state.completion_suggestions(Position { line, character })
}

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
fn suggests_batch_tool_import_properties() {
    let completion_suggestions = inline_completion_suggestions! {
        from mcp.local.tool {
            <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "input", "bindings", "max_calls", "output", "tool");
    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Agent);
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

    assert_eq!(completion_suggestion.insert_text, "participants: [{  }]");
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
fn reports_invalid_mcp_batch_common_schema_field() {
    let source = inline_document_template! {
        from mcp.local.tool {
            output {
                participants: [{  }]
            }

            tool list_all_participants_who_has_answered_given_task
            tool update_user_name
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("update_user_name")));
}

#[test]
fn reports_invalid_mcp_tool_override_schema_field_type() {
    let source = inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                output {
                    shared: number
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("must be `string`")));
}

#[test]
fn reports_invalid_mcp_tool_binding_override_property() {
    let source = inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                bindings {
                    unknown: 123
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown")));
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
        "output {\n    participants: [{  }]\n    shared: string\n}"
    );
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
    assert!(code_actions[0].edit.new_text.contains("participants: [{  }]"));
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
fn suggests_types_inside_tool_bounded_field() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            bindings {
                project: <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        "description",
        "input",
        "bindings",
    );
}

#[test]
fn suggests_bounded_arguments_inside_tool_call() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }

            bindings {
                password: string
                token: string
            }
        }

        agent tooling {
            uses: [tool.knowledge_base_search {
                bindings {
                    <cursor>
                }
            }]
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "password", "token");
    assert_completion_excludes_labels!(&completion_suggestions, "query");
}

#[test]
fn suggests_declared_bindings_inside_deterministic_tool_call_binding_overrides() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }

            bindings {
                password: string
                endpoint: "https://example.test"
                token: string
            }
        }

        dynamic {
            search_result: call tool.knowledge_base_search {
                bindings {
                    <cursor>
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "password", "token");
    assert_completion_excludes_labels!(&completion_suggestions, "query", "endpoint");
}

#[test]
fn filters_existing_bindings_inside_deterministic_tool_call_binding_overrides() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            bindings {
                password: string
                token: string
            }
        }

        dynamic {
            search_result: call tool.knowledge_base_search {
                bindings {
                    password: input.password
                    <cursor>
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "token");
    assert_completion_excludes_labels!(&completion_suggestions, "password");
}
