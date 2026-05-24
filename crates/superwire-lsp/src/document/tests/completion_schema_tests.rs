use super::*;

#[test]
fn completes_schema_references_in_type_context() {
    let completion_suggestions = inline_completion_suggestions! {
        schema person {
            name: string
        }

        input {
            profile: schema.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "person");
}

#[test]
fn completes_schema_enum_fields_in_type_context() {
    let completion_suggestions = inline_completion_suggestions! {
        schema main {
            language_enum: enum { en_US, zh_CN, fr }
            plain_language: string
        }

        tool example {
            input {
                language: schema.main.<cursor>
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "language_enum");
    assert_completion_excludes_labels!(&completion_suggestions, "plain_language");
}

#[test]
fn uses_schema_field_description_in_reference_completion_documentation() {
    let completion_suggestions = inline_completion_suggestions! {
        schema person {
            /// first name from schema
            first_name: string
            age: number
        }

        input {
            profile: schema.person
        }

        agent writer {
            instruction: "hello {{ input.profile.<cursor> }}"
            output {
                value: string
            }
        }
    };

    let first_name_completion = completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == "first_name")
        .expect("first_name completion should exist");

    assert_eq!(first_name_completion.documentation, "first name from schema");
}

#[test]
fn excludes_current_schema_from_schema_type_suggestions() {
    let completion_suggestions = inline_completion_suggestions! {
        schema person {
            related: schema.<cursor>
        }

        schema team {
            members: [string]
        }
    };

    assert_completion_contains!(&completion_suggestions, "team");
    assert_completion_excludes_labels!(&completion_suggestions, "person");
}

#[test]
fn excludes_current_schema_from_schema_type_suggestions_with_parse_errors() {
    let completion_suggestions = inline_completion_suggestions! {
        schema person {
            related: schema.<cursor>
        }

        @
    };

    assert_completion_excludes_labels!(&completion_suggestions, "person");
    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_type_suggestions_after_non_schema_dot_access() {
    let completion_suggestions = inline_completion_suggestions! {
        schema test {
            test: boolean.<cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_key_suggestions_inside_input_block() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            <cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_key_suggestions_inside_output_object_literal() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            example: {
                <cursor>
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_key_suggestions_inside_agent_output_object_literal() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                <cursor>
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_types_inside_agent_output_object_field_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                result: <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider, DeclarationKeyword::Agent);
}

#[test]
fn suggests_array_type_for_agent_output_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                values: <cursor>
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "[string]");
}

#[test]
fn suppresses_agent_property_suggestions_inside_agent_output_object_field_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                property: <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Uses,
        AgentExpressionPropertyName::Instruction
    );
}

#[test]
fn suggests_only_types_inside_nested_agent_output_object_field_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                property: {
                    id: <cursor>
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Uses,
        DeclarationKeyword::Provider
    );
}

#[test]
fn suppresses_suggestions_inside_agent_output_array_fixed_length_slot() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output {
                items: [string; <cursor>]
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_types_for_input_field_values() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            product_name: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider, DeclarationKeyword::Agent);
}
