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
fn suggests_bounded_arguments_inside_tool_call() {
    let completion_suggestions = inline_completion_suggestions! {
        tool knowledge_base_search {
            input {
                query: string
            }

            bounded {
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
