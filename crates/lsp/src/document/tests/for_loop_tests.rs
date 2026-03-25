use super::*;

#[test]
fn completes_input_fields_in_for_loop_iterable_reference() {
    let (source, cursor_position) = inline_document_with_cursor! {
        input {
            products: [string]
        }

        agent worker for item in input.<cursor> {
            prompt: item
        }
    };

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains!(&completion_suggestions, "products");
}

#[test]
fn suppresses_non_iterable_input_field_suggestions_in_for_loop_iterable_reference() {
    let (source, cursor_position) = inline_document_with_cursor! {
        input {
            xxxx: string
        }

        agent worker for item in input.<cursor> {
            prompt: item
        }
    };

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_agent_properties_inside_for_loop_agent_block() {
    let (source, cursor_position) = inline_document_with_cursor! {
        agent source {}

        agent worker for item in agent.source {
            <cursor>
        }
    };

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains!(&completion_suggestions, AgentExpressionPropertyName::Prompt);
    assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
}
