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

#[test]
fn suggests_for_keyword_after_agent_name_in_agent_header() {
    let source = ["agent remediation_plan  {", "}"].join("\n");
    let cursor_offset = source
        .find("agent remediation_plan ")
        .expect("test source should contain agent declaration prefix")
        + "agent remediation_plan ".len();
    let cursor_position = position_from_source_offset(&source, cursor_offset);
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::For);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        AgentExpressionPropertyName::Prompt
    );

    let for_keyword_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == ForClauseKeyword::For.as_str())
        .expect("for keyword completion should exist");

    assert!(matches!(for_keyword_completion.kind, CompletionKind::Keyword));
}

#[test]
fn suggests_in_keyword_after_for_iterator_name_in_agent_header() {
    let source = ["agent remediation_plan for item  {", "}"].join("\n");
    let cursor_offset = source
        .find("agent remediation_plan for item ")
        .expect("test source should contain for-loop binding prefix")
        + "agent remediation_plan for item ".len();
    let cursor_position = position_from_source_offset(&source, cursor_offset);
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::In);
    assert_completion_excludes_labels!(&completion_suggestions, ForClauseKeyword::For, DeclarationKeyword::Agent);

    let in_keyword_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == ForClauseKeyword::In.as_str())
        .expect("in keyword completion should exist");

    assert!(matches!(in_keyword_completion.kind, CompletionKind::Keyword));
}

fn position_from_source_offset(source_text: &str, source_offset: usize) -> Position {
    let mut line = 0_u32;
    let mut character = 0_u32;

    for character_in_source in source_text[..source_offset].chars() {
        if character_in_source == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    Position { line, character }
}
