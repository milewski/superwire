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
    let mut completion_matrix_cases = Vec::new();

    completion_matrix_cases.extend(declaration_completion_matrix_cases());
    completion_matrix_cases.extend(agent_property_completion_matrix_cases());
    completion_matrix_cases.extend(inference_completion_matrix_cases());
    completion_matrix_cases.extend(typed_declaration_completion_matrix_cases());
    completion_matrix_cases.extend(interpolation_completion_matrix_cases());
    completion_matrix_cases.extend(for_loop_completion_matrix_cases());
    completion_matrix_cases.extend(tools_completion_matrix_cases());

    completion_matrix_cases
}

fn declaration_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
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
            expected_present_labels: vec![
                DeclarationKeyword::Provider.as_str(),
                DeclarationKeyword::Agent.as_str(),
                DeclarationKeyword::Resource.as_str(),
                DeclarationKeyword::Prompt.as_str(),
            ],
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
    ]
}

fn agent_property_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
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
                AgentExpressionPropertyName::Instruction.as_str(),
            ],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "inference_object_excludes_agent_properties",
            context: CompletionMatrixContext::AgentProperties,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                model fast from openai {
                    inference {
                        <cursor>
                    }
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![
                AgentExpressionPropertyName::Model.as_str(),
                AgentExpressionPropertyName::Instruction.as_str(),
            ],
            expects_empty_suggestions: false,
        },
    ]
}

fn inference_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "inference_object_suggests_inference_settings",
            context: CompletionMatrixContext::InferenceBlock,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                agent writer {
                    model: model.fast {
                        inference {
                            <cursor>
                        }
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
                    model: model.openai_model

                    <cursor>

                    model: model.fast {
                        inference {
                            temperature: 0.2
                            max_tokens: 12_000
                        }
                    }
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec![InferenceSetting::Temperature.key(), InferenceSetting::MaxTokens.key()],
            expects_empty_suggestions: false,
        },
    ]
}

fn typed_declaration_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "typed_declaration_suggests_primitive_types",
            context: CompletionMatrixContext::TypedDeclarations,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                input {
                    product_name: <cursor>
                }
            },
            expected_present_labels: vec![TypeExpression::String.completion_label(), TypeExpression::Number.completion_label()],
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
            expected_absent_labels: vec![TypeExpression::String.completion_label(), TypeExpression::Number.completion_label()],
            expects_empty_suggestions: true,
        },
    ]
}

fn interpolation_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "interpolation_suggests_agent_references",
            context: CompletionMatrixContext::Interpolation,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                provider openai from openai {}

                model openai_model from openai {
                    id: "gpt-4.1-mini"
                }

                agent context_agent {
                    model: model.openai_model
                    instruction: "hello"
                    output {
                value: string
            }
                }

                agent worker {
                    model: model.openai_model
                    instruction: "example {{ agent.<cursor> }}"
                    output {
                value: string
            }
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
                provider openai from openai {}

                model openai_model from openai {
                    id: "gpt-4.1-mini"
                }

                agent context_agent {
                    model: model.openai_model
                    instruction: "hello"
                    output {
                value: string
            }
                }

                agent worker {
                    model: model.openai_model
                    instruction: "example {{ agent.<cursor> }}"
                    output {
                value: string
            }
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec!["worker"],
            expects_empty_suggestions: false,
        },
    ]
}

fn for_loop_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "for_loop_iterable_suggests_iterable_fields",
            context: CompletionMatrixContext::ForLoopIterable,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                input {
                    products: [string]
                }

                agent worker for item in input.<cursor> {
                    instruction: item
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
                    instruction: item
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec!["product_name"],
            expects_empty_suggestions: true,
        },
    ]
}

fn tools_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
        CompletionMatrixCase {
            case_name: "uses_expression_suggests_tool_keyword",
            context: CompletionMatrixContext::Tools,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                agent tooling {
                    uses: <cursor>
                }
            },
            expected_present_labels: vec![ReferenceKeyword::Tool.as_str()],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "tool_namespace_suggests_declared_tools",
            context: CompletionMatrixContext::Tools,
            expectation_kind: CompletionExpectationKind::Positive,
            source_template: inline_document_template! {
                tool knowledge_base_search {
                    query: string
                }

                agent tooling {
                    uses: [tool.<cursor>]
                }
            },
            expected_present_labels: vec!["knowledge_base_search"],
            expected_absent_labels: vec![],
            expects_empty_suggestions: false,
        },
        CompletionMatrixCase {
            case_name: "tool_call_excludes_fixed_bindings_and_input_field_arguments",
            context: CompletionMatrixContext::Tools,
            expectation_kind: CompletionExpectationKind::Negative,
            source_template: inline_document_template! {
                tool knowledge_base_search {
                    input {
                        query: string
                    }

                    bindings {
                        password: "secret"
                    }
                }

                agent tooling {
                    uses: [tool.knowledge_base_search {
                        bindings {
                            <cursor>
                        }
                    }]
                }
            },
            expected_present_labels: vec![],
            expected_absent_labels: vec!["password", "query"],
            expects_empty_suggestions: false,
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
