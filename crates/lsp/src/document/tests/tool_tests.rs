use super::*;

#[test]
fn suggests_tool_keyword_inside_tools_expression_context() {
    let completion_suggestions = completion_suggestions_from_template(
        r#"
            agent tooling {
                tools: <cursor>
            }
            "#,
    );

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Tool);
}

#[test]
fn suppresses_member_suggestions_for_tool_namespace_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent tooling {
            tools: [tool.<cursor>]
        }
    };

    assert!(completion_suggestions.is_empty());
}
