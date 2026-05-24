use super::*;

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
fn inserts_plain_tool_name_for_tool_with_fixed_bindings() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            bindings {
                password: "secret"
            }
        }

        agent tooling {
            uses: [tool.<cursor>]
        }
    };

    let completion_suggestion = completion_suggestion_by_label(&completion_suggestions, "issue_tracker_lookup");

    assert_eq!(completion_suggestion.insert_text, "issue_tracker_lookup");
}

#[test]
fn does_not_complete_tool_with_invalid_schema_like_bindings() {
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

    assert_completion_excludes_labels!(&completion_suggestions, "issue_tracker_lookup");
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
fn does_not_suggest_types_inside_tool_binding_value() {
    let completion_suggestions = inline_completion_suggestions! {
        tool issue_tracker_lookup {
            bindings {
                project: <cursor>
            }
        }
    };

    assert_completion_excludes_labels!(
        &completion_suggestions,
        TypeExpression::String,
        TypeExpression::Number,
        "description",
        "bindings",
    );
}

#[test]
fn does_not_suggest_fixed_bindings_inside_tool_call() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }

            bindings {
                password: "secret"
                token: "token"
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

    assert_completion_excludes_labels!(&completion_suggestions, "password", "token", "query");
}

#[test]
fn does_not_suggest_fixed_bindings_inside_deterministic_tool_call_binding_overrides() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }

            bindings {
                password: "secret"
                endpoint: "https://example.test"
                token: "token"
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

    assert_completion_excludes_labels!(&completion_suggestions, "password", "token", "query", "endpoint");
}

#[test]
fn does_not_suggest_existing_fixed_bindings_inside_deterministic_tool_call_binding_overrides() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            bindings {
                password: "secret"
                token: "token"
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

    assert_completion_excludes_labels!(&completion_suggestions, "password", "token");
}
