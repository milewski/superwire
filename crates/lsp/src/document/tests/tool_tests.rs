use super::*;
use std::collections::BTreeMap;
use superwire_core::mcp::{McpLock, McpServerLock, McpToolLock};

#[test]
fn suggests_tool_keyword_inside_tools_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        agent tooling {
            tools: <cursor>
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
            tools: [tool.<cursor>]
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
            tools: [
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
            tools: [tool.<cursor>]
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
            tools: [tool.<cursor>]
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
            tools: [tool.<cursor> {
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

    assert_completion_contains_labels!(&completion_suggestions, "description", "using", "input", "bindings", "output");
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
fn suggests_mcp_source_inside_tool_using_property() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            using: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "mcp.");
    assert_completion_excludes_labels!(&completion_suggestions, "input", "bindings", "output");
}

#[test]
fn uses_mcp_lock_for_tool_schema_and_source_completion() {
    let source_template = inline_document_template! {
        mcp local {
            endpoint: "http://docker.localhost/mcp/project"
        }

        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        tool update_user_name {
            using: mcp.local.<cursor>
        }

        agent tooling {
            model: openai("gpt-4.1-mini")
            tools: [tool.update_user_name]
            prompt: "Rename the user"
            output: string
        }
    };
    let (source, cursor_position) = source_with_cursor(source_template);
    let document_state = DocumentState::new(source, Some(test_mcp_lock()));
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains_labels!(
        &completion_suggestions,
        "list_all_participants_who_has_answered_given_task",
        "update-user-name"
    );

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

        tool update_user_name {
            using: mcp.local.update-user-name
        }

        agent tooling {
            model: openai("gpt-4.1-mini")
            tools: [tool.update_user_name]
            prompt: "Rename the user"
            output: string
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));

    assert!(document_state.diagnostics().is_empty());
}

fn test_mcp_lock() -> McpLock {
    let mut tools = BTreeMap::new();
    tools.insert(
        "list_all_participants_who_has_answered_given_task".to_string(),
        McpToolLock {
            name: "list_all_participants_who_has_answered_given_task".to_string(),
            description: Some("List all participants who answered a task".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "number" },
                    "task_id": { "type": "number" }
                },
                "required": ["project_id", "task_id"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "participants": { "type": "array", "items": { "type": "object" } }
                },
                "required": ["participants"]
            })),
        },
    );
    tools.insert(
        "update-user-name".to_string(),
        McpToolLock {
            name: "update-user-name".to_string(),
            description: Some("Update a user name".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "user_name": { "type": "string" }
                },
                "required": ["user_name"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                },
                "required": ["success"]
            })),
        },
    );
    let mut servers = BTreeMap::new();
    servers.insert("local".to_string(), McpServerLock { tools });

    McpLock { servers }
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
            tools: [tool.knowledge_base_search {
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
