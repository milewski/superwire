use super::*;

#[test]
fn suggests_tool_keyword_inside_tools_expression_context() {
    let (source, cursor_position) = source_with_cursor(
        r#"
            agent tooling {
                tools: <cursor>
            }
            "#,
    );

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Tool);
}

#[test]
fn suppresses_member_suggestions_for_tool_namespace_reference() {
    let (source, cursor_position) = inline_document_with_cursor! {
        agent tooling {
            tools: [tool.<cursor>]
        }
    };

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert!(completion_suggestions.is_empty());
}
