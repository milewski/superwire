use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionMatrixContext {
    Declarations,
    AgentProperties,
    InferenceBlock,
    TypedDeclarations,
    Interpolation,
    ForLoopIterable,
    Tools,
}

impl CompletionMatrixContext {
    fn all() -> [Self; 7] {
        [
            Self::Declarations,
            Self::AgentProperties,
            Self::InferenceBlock,
            Self::TypedDeclarations,
            Self::Interpolation,
            Self::ForLoopIterable,
            Self::Tools,
        ]
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Declarations => "declarations",
            Self::AgentProperties => "agent_properties",
            Self::InferenceBlock => "inference_block",
            Self::TypedDeclarations => "typed_declarations",
            Self::Interpolation => "interpolation",
            Self::ForLoopIterable => "for_loop_iterable",
            Self::Tools => "tools",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionExpectationKind {
    Positive,
    Negative,
}

impl CompletionExpectationKind {
    fn display_name(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
        }
    }
}

struct CompletionMatrixCase {
    case_name: &'static str,
    context: CompletionMatrixContext,
    expectation_kind: CompletionExpectationKind,
    source_template: &'static str,
    expected_present_labels: Vec<&'static str>,
    expected_absent_labels: Vec<&'static str>,
    expects_empty_suggestions: bool,
}

fn completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "top_level_declares_keywords",
            context: CompletionMatrixContext::Declarations,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                <cursor>

                output {
                    value: null
                }
            },
            expected_present_labels: vec![DeclarationKeyword::Provider.as_str(), DeclarationKeyword::Agent.as_str()],
            expected_absent_labels: vec![BuiltinFunctionName::Context.as_str()],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "agent_block_excludes_declaration_keywords",
            context: CompletionMatrixContext::Declarations,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                agent writer {
                    <cursor>
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![DeclarationKeyword::Provider.as_str(), DeclarationKeyword::Schema.as_str()],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "agent_block_suggests_agent_properties",
            context: CompletionMatrixContext::AgentProperties,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                agent writer {
                    <cursor>
                }
            },
            expected_present_labels: vec![
                AgentExpressionPropertyName::Model.as_str(),
                AgentExpressionPropertyName::Prompt.as_str(),
            ],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "inference_object_excludes_agent_properties",
            context: CompletionMatrixContext::AgentProperties,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                agent writer {
                    inference: {
                        <cursor>
                    }
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![
                AgentExpressionPropertyName::Model.as_str(),
                AgentExpressionPropertyName::Prompt.as_str(),
            ],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "inference_object_suggests_inference_settings",
            context: CompletionMatrixContext::InferenceBlock,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                agent writer {
                    inference: {
                        <cursor>
                    }
                }
            },
            expected_present_labels: vec![InferenceSetting::Temperature.key(), InferenceSetting::MaxTokens.key()],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "agent_scope_excludes_inference_settings",
            context: CompletionMatrixContext::InferenceBlock,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                agent release_analyst {
                    model: openai("gpt-4.1-mini")

                    <cursor>

                    inference: {
                        temperature: 0.2
                        max_tokens: 12_000
                    }
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![InferenceSetting::Temperature.key(), InferenceSetting::MaxTokens.key()],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "typed_declaration_suggests_primitive_types",
            context: CompletionMatrixContext::TypedDeclarations,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                input {
                    product_name: <cursor>
                }
            },
            expected_present_labels: vec![
                TypeExpression::String.as_completion_label(),
                TypeExpression::Number.as_completion_label(),
            ],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "input_key_position_excludes_typed_declaration_values",
            context: CompletionMatrixContext::TypedDeclarations,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                input {
                    <cursor>
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![
                TypeExpression::String.as_completion_label(),
                TypeExpression::Number.as_completion_label(),
            ],
            expects_empty_suggestions: true,
        },
        CompletionMatrixCase {
            case_name: "interpolation_suggests_agent_references",
            context: CompletionMatrixContext::Interpolation,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                provider openai {
                    driver: "openai"
                    models: ["gpt-4.1-mini"]
                }

                agent context_agent {
                    model: openai("gpt-4.1-mini")
                    prompt: "hello"
                    output: string
                }

                agent worker {
                    model: openai("gpt-4.1-mini")
                    prompt: "example {{ agent.<cursor> }}"
                    output: string
                }
            },
            expected_present_labels: vec!["context_agent"],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "interpolation_excludes_current_agent_reference",
            context: CompletionMatrixContext::Interpolation,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                provider openai {
                    driver: "openai"
                    models: ["gpt-4.1-mini"]
                }

                agent context_agent {
                    model: openai("gpt-4.1-mini")
                    prompt: "hello"
                    output: string
                }

                agent worker {
                    model: openai("gpt-4.1-mini")
                    prompt: "example {{ agent.<cursor> }}"
                    output: string
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec!["worker"],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "for_loop_iterable_suggests_iterable_fields",
            context: CompletionMatrixContext::ForLoopIterable,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                input {
                    products: [string]
                }

                agent worker for item in input.<cursor> {
                    prompt: item
                }
            },
            expected_present_labels: vec!["products"],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "for_loop_iterable_excludes_non_iterable_fields",
            context: CompletionMatrixContext::ForLoopIterable,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                input {
                    product_name: string
                }

                agent worker for item in input.<cursor> {
                    prompt: item
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec!["product_name"],
            expects_empty_suggestions: true,
        },
        CompletionMatrixCase {
            case_name: "tools_expression_suggests_tool_keyword",
            context: CompletionMatrixContext::Tools,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                agent tooling {
                    tools: <cursor>
                }
            },
            expected_present_labels: vec![ReferenceKeyword::Tool.as_str()],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "tool_namespace_excludes_member_suggestions",
            context: CompletionMatrixContext::Tools,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                agent tooling {
                    tools: [tool.<cursor>]
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![],
            expects_empty_suggestions: true,
        },
    ]
}

#[test]
fn completion_behavior_matrix_covers_primary_contexts() {
    let completion_matrix_cases = completion_matrix_cases();

    for completion_matrix_context in CompletionMatrixContext::all() {
        assert!(
            completion_matrix_cases.iter().any(|completion_matrix_case| {
                completion_matrix_case.context == completion_matrix_context
                    && completion_matrix_case.expectation_kind == CompletionExpectationKind::Positive
            }),
            "completion matrix should include a positive case for context {}",
            completion_matrix_context.display_name()
        );

        assert!(
            completion_matrix_cases.iter().any(|completion_matrix_case| {
                completion_matrix_case.context == completion_matrix_context
                    && completion_matrix_case.expectation_kind == CompletionExpectationKind::Negative
            }),
            "completion matrix should include a negative case for context {}",
            completion_matrix_context.display_name()
        );
    }

    for completion_matrix_case in completion_matrix_cases {
        let completion_suggestions = completion_suggestions_from_template(completion_matrix_case.source_template);
        let available_labels = completion_label_set(&completion_suggestions);
        let mut sorted_available_labels = available_labels.into_iter().collect::<Vec<_>>();

        sorted_available_labels.sort_unstable();

        if completion_matrix_case.expects_empty_suggestions {
            assert!(
                completion_suggestions.is_empty(),
                "case `{}` ({}/{}) expected empty completion suggestions; got labels {:?}",
                completion_matrix_case.case_name,
                completion_matrix_case.context.display_name(),
                completion_matrix_case.expectation_kind.display_name(),
                sorted_available_labels
            );

            continue;
        }

        for expected_label in completion_matrix_case.expected_present_labels {
            assert!(
                sorted_available_labels.contains(&expected_label),
                "case `{}` ({}/{}) expected label `{}`; available labels {:?}",
                completion_matrix_case.case_name,
                completion_matrix_case.context.display_name(),
                completion_matrix_case.expectation_kind.display_name(),
                expected_label,
                sorted_available_labels
            );
        }

        for unexpected_label in completion_matrix_case.expected_absent_labels {
            assert!(
                !sorted_available_labels.contains(&unexpected_label),
                "case `{}` ({}/{}) should not include label `{}`; available labels {:?}",
                completion_matrix_case.case_name,
                completion_matrix_case.context.display_name(),
                completion_matrix_case.expectation_kind.display_name(),
                unexpected_label,
                sorted_available_labels
            );
        }
    }
}

#[test]
fn completes_nested_input_field_attributes() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            profile: {
                name: {
                    first: string
                    last: string
                }
            }
        }

        output {
            value: input.profile.name.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "first", "last");
}

#[test]
fn completes_provider_driver_specific_properties() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "endpoint", "api_key");
}

#[test]
fn suppresses_builtin_functions_in_top_level_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        <cursor>

        output {
            value: null
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        DeclarationKeyword::Schema,
        SingletonDeclarationKind::Input,
        SingletonDeclarationKind::Secrets
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        SingletonDeclarationKind::Output,
        "tool",
        "string",
        "number"
    );
    assert_completion_excludes_labels!(&completion_suggestions, BuiltinFunctionName);
    assert!(completion_suggestions
        .iter()
        .all(|completion_suggestion| matches!(completion_suggestion.kind, CompletionKind::Keyword)));
}

#[test]
fn suppresses_existing_singleton_keywords_in_top_level_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        <cursor>

        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        input {
            prompt: string
        }

        secrets {
            api_key: string
        }

        output {
            value: null
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        DeclarationKeyword::Schema
    );

    assert_completion_excludes_labels!(
        &completion_suggestions,
        SingletonDeclarationKind::Input,
        SingletonDeclarationKind::Secrets,
        SingletonDeclarationKind::Output,
        "tool",
        "string",
        "number"
    );
}

#[test]
fn suppresses_suggestions_after_singleton_declaration_keyword_header() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        input <cursor>
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_suggestions_after_named_declaration_keyword_header() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent <cursor>
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_builtin_functions_in_output_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            value: <cursor>
        }
    };

    assert_completion_contains_label_groups!(&completion_suggestions, BuiltinFunctionName);
}

#[test]
fn suggests_only_agent_properties_in_agent_block_scope() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            <cursor>
        }
    };

    assert_completion_contains_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Model,
        AgentExpressionPropertyName::Prompt,
        AgentExpressionPropertyName::Tools,
        "output"
    );

    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Function);
}

#[test]
fn suggests_only_context_function_for_agent_context_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent example {
            context: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, BuiltinFunctionName::Context);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        BuiltinFunctionName::Template,
        BuiltinFunctionName::Compact,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Tool,
        "string",
        "number"
    );

    assert_eq!(completion_suggestions.len(), 1);
}

#[test]
fn suggests_only_inference_settings_inside_inference_object() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            inference: {
                <cursor>
            }
        }
    };

    assert_completion_contains_all_inference_settings!(&completion_suggestions);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Model,
        DeclarationKeyword::Provider
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Function);
}

#[test]
fn suppresses_inference_suggestions_inside_string_literal_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            inference: {
                max_tokens: "<cursor>"
            }
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_agent_properties_before_inference_block() {
    let completion_suggestions = inline_completion_suggestions! {
        agent release_analyst {
            model: openai("gpt-4.1-mini")

            <cursor>

            inference: {
                temperature: 0.2
                max_tokens: 12_000
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, AgentExpressionPropertyName::Prompt);
    assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
}

#[test]
fn includes_descriptive_details_for_agent_and_inference_completions() {
    let (agent_source, agent_cursor_position) = inline_document_with_cursor! {
        agent writer {
            <cursor>
        }
    };

    let (inference_source, inference_cursor_position) = inline_document_with_cursor! {
        agent writer {
            inference: {
                <cursor>
            }
        }
    };

    let agent_completions = completion_suggestions_from_source(agent_source, agent_cursor_position);
    let inference_completions = completion_suggestions_from_source(inference_source, inference_cursor_position);

    let model_completion = agent_completions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == "model")
        .expect("agent completion should include model property");

    let max_tokens_completion = inference_completions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == InferenceSetting::MaxTokens.key())
        .expect("inference completion should include max_tokens setting");

    assert_eq!(model_completion.detail, "Model binding (required)");
    assert_eq!(max_tokens_completion.detail, "Token budget (integer)");
}

#[test]
fn completes_registered_provider_models_inside_model_call() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini", "gpt-4o-mini"]
        }

        agent writer {
            model: openai("<cursor>")
            prompt: "hello"
            output: string
        }
    };

    assert_completion_contains!(&completion_suggestions, "gpt-4.1-mini", "gpt-4o-mini");
}

#[test]
fn suppresses_fallback_suggestions_after_terminal_agent_output_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            prompt: "hello"
            output: string
        }

        output {
            greeting: agent.greeting.<cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_agent_output_fields_for_nested_agent_output_reference() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            prompt: "hello"
            output: {
                message: string
                language: string
            }
        }

        output {
            greeting: agent.greeting.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "message", "language");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Agent,
        BuiltinFunctionName::Context,
        "string",
        "number"
    );
}

#[test]
fn completes_schema_references_in_type_context() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            name: string
        }

        input {
            profile: schema.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "Person");
}

#[test]
fn excludes_current_schema_from_schema_type_suggestions() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            related: schema.<cursor>
        }

        schema Team {
            members: [string]
        }
    };

    assert_completion_contains!(&completion_suggestions, "Team");
    assert_completion_excludes_labels!(&completion_suggestions, "Person");
}

#[test]
fn excludes_current_schema_from_schema_type_suggestions_with_parse_errors() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            related: schema.<cursor>
        }

        @
    };

    assert_completion_excludes_labels!(&completion_suggestions, "Person");
    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_type_suggestions_after_non_schema_dot_access() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Test {
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
fn suggests_only_types_for_input_field_values() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            product_name: <cursor>
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider, DeclarationKeyword::Agent);
}
