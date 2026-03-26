use super::*;

#[test]
fn completes_input_fields_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            products: [string]
        }

        agent worker for item in input.<cursor> {
            prompt: item
        }
    };

    assert_completion_contains!(&completion_suggestions, "products");
}

#[test]
fn suppresses_non_iterable_input_field_suggestions_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            xxxx: string
        }

        agent worker for item in input.<cursor> {
            prompt: item
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_agent_properties_inside_for_loop_agent_block() {
    let completion_suggestions = inline_completion_suggestions! {
        agent source {}

        agent worker for item in agent.source {
            <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, AgentExpressionPropertyName::Prompt);
    assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
}
