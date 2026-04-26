use super::*;

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
fn inserts_call_for_tool_with_bounded_arguments_when_parentheses_do_not_exist() {
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

    assert_eq!(completion_suggestion.insert_text, "issue_tracker_lookup($1)");
}

#[test]
fn inserts_plain_tool_name_when_call_parentheses_already_exist() {
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
            tools: [tool.<cursor>(password: secrets.knowledge_base_password)]
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

    assert_completion_contains_labels!(&completion_suggestions, "description", "input", "bindings");
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
            tools: [tool.knowledge_base_search(<cursor>)]
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, "password", "token");
    assert_completion_excludes_labels!(&completion_suggestions, "query");
}
