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
fn completes_agent_names_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings_source {
            output: [string]
        }

        agent remediation_plan for finding in agent.<cursor> {
            prompt: finding
        }
    };

    assert_completion_contains!(&completion_suggestions, "findings_source");
    assert_completion_excludes_labels!(&completion_suggestions, "remediation_plan");
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
fn completes_iterable_secrets_fields_in_for_loop_iterable_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        secrets {
            finding_ids: [string]
            api_key: string
        }

        agent remediation_plan for finding in secrets.<cursor> {
            prompt: finding
        }
    };

    assert_completion_contains!(&completion_suggestions, "finding_ids");
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
fn suggests_inference_settings_inside_for_loop_agent_inference_object() {
    let completion_suggestions = inline_completion_suggestions! {
        agent number_note for number in [1, 2, 3, 4] {
            inference: {
                <cursor>
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);
    assert_completion_excludes_labels!(&completion_suggestions, AgentExpressionPropertyName::Tools);
}

#[test]
fn suggests_for_loop_iterator_inside_prompt_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            numbers: [number]
        }

        agent input_number_note for n in input.numbers {
            model: ollama("qwen3:8b")
            prompt: "Write a short note for input number {{ <cursor> }}"
            output: {
                number: number
                note: string
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "n");
}

#[test]
fn suggests_only_valid_iterable_values_after_for_in_clause() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan for something in <cursor> {
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets,
        "[]"
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        "boolean",
        "number",
        ReferenceKeyword::Tool,
        BuiltinFunctionName::Context
    );
}

#[test]
fn suggests_for_keyword_after_agent_name_in_agent_header() {
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan <cursor> {
        }
    };

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
    let completion_suggestions = inline_completion_suggestions! {
        agent remediation_plan for item <cursor> {
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ForClauseKeyword::In);
    assert_completion_excludes_labels!(&completion_suggestions, ForClauseKeyword::For, DeclarationKeyword::Agent);

    let in_keyword_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == ForClauseKeyword::In.as_str())
        .expect("in keyword completion should exist");

    assert!(matches!(in_keyword_completion.kind, CompletionKind::Keyword));
}
