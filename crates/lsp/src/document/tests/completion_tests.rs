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
    ]
}

fn tools_completion_matrix_cases() -> Vec<CompletionMatrixCase> {
    vec![
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
fn suggests_array_literals_for_provider_models_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        provider ollama {
            driver: "ollama"
            models: <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "[]", "[\"\"]");
}

#[test]
fn suppresses_suggestions_inside_provider_model_name_string_literal() {
    let completion_suggestions = inline_completion_suggestions! {
        provider ollama {
            driver: "ollama"
            models: ["<cursor>"]
        }
    };

    assert!(completion_suggestions.is_empty());
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
fn suggests_only_block_braces_after_named_schema_declaration_name() {
    let completion_suggestions = inline_completion_suggestions! {
        schema test <cursor> {
        }
    };

    assert_completion_contains!(&completion_suggestions, "{}");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        ReferenceKeyword::Tool,
        "number",
        "string"
    );

    assert_eq!(completion_suggestions.len(), 1);
}

#[test]
fn suggests_only_valid_output_value_roots_and_literals_in_output_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            value: <cursor>
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets
    );
    assert_completion_contains!(&completion_suggestions, "{}", "[]", "\"\"", "0", "true", "false", "null");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        ReferenceKeyword::Tool
    );
}

#[test]
fn suppresses_invalid_reference_roots_in_output_expression_context() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            value: schema.<cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_only_valid_output_values_for_root_output_field() {
    let completion_suggestions = inline_completion_suggestions! {
        output {
            manual_numbers: <cursor>
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets
    );
    assert_completion_contains!(&completion_suggestions, "{}", "[]", "\"\"", "0");
    assert_completion_excludes_labels!(
        &completion_suggestions,
        "number",
        "string",
        ReferenceKeyword::Tool,
        DeclarationKeyword::Provider
    );
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
fn suggests_only_valid_prompt_value_roots_and_literals() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            prompt: <cursor>
            output: string
        }
    };

    assert_completion_contains!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        "\"\"",
        "\"\"\""
    );
    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        ReferenceKeyword::Tool,
        DeclarationKeyword::Provider,
        DeclarationKeyword::Schema,
        "number",
        "string"
    );

    let single_line_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"");
    let multiline_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"\"");

    assert_eq!(single_line_string_completion.insert_text, "\"\"");
    assert_eq!(multiline_string_completion.insert_text, "\"\"\"\n\"\"\"");
}

#[test]
fn uses_current_line_indentation_for_multiline_prompt_literal_completion() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            prompt: <cursor>
            output: string
        }
    });

    let prompt_line = source
        .lines()
        .nth(usize::try_from(cursor_position.line).expect("cursor line should fit usize"))
        .expect("prompt line should exist");

    let prompt_line_indentation = prompt_line
        .char_indices()
        .find_map(|(character_offset, character)| (!character.is_whitespace()).then_some(character_offset))
        .map(|first_non_whitespace_offset| &prompt_line[..first_non_whitespace_offset])
        .unwrap_or_default();

    let expected_multiline_insert_text = format!("\"\"\"\n{prompt_line_indentation}\"\"\"");

    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);
    let multiline_string_completion = completion_suggestion_by_label(&completion_suggestions, "\"\"\"");

    assert_eq!(multiline_string_completion.insert_text, expected_multiline_insert_text);
}

#[test]
fn suppresses_invalid_prompt_reference_roots() {
    let completion_suggestions = inline_completion_suggestions! {
        agent writer {
            prompt: secrets.<cursor>
            output: string
        }
    };

    assert!(completion_suggestions.is_empty());
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
fn suggests_only_inference_value_reference_roots_for_integer_setting() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            max_tokens: number
            label: string
        }

        schema Limits {
            max_tokens: number
        }

        agent helper {
            prompt: "hello"
            output: {
                max_tokens: number
                label: string
            }
        }

        agent writer {
            inference: {
                max_tokens: <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Agent, ReferenceKeyword::Input);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Provider,
        "string",
        "number"
    );
}

#[test]
fn suggests_only_numeric_resolving_input_fields_for_integer_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            max_tokens: number
            metadata: {
                nested_limit: number
            }
            label: string
        }

        agent writer {
            inference: {
                max_tokens: input.<cursor>
            }
        }
    };

    assert_completion_contains!(&completion_suggestions, "max_tokens", "metadata");
    assert_completion_excludes_labels!(&completion_suggestions, "label");
}

#[test]
fn suppresses_schema_reference_suggestions_for_integer_inference_value() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Limits {
            max_tokens: number
        }

        agent writer {
            inference: {
                max_tokens: schema.<cursor>
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
    let agent_completions = inline_completion_suggestions! {
        agent writer {
            <cursor>
        }
    };

    let inference_completions = inline_completion_suggestions! {
        agent writer {
            inference: {
                <cursor>
            }
        }
    };

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

    let gpt_model_completion = completion_suggestion_by_label(&completion_suggestions, "gpt-4.1-mini");

    assert_eq!(gpt_model_completion.insert_text, "gpt-4.1-mini");
}

#[test]
fn suggests_only_declared_providers_for_model_property_value() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        provider anthropic {
            driver: "anthropic"
            models: ["claude-3-7-sonnet-latest"]
        }

        agent writer {
            model: <cursor>
            prompt: "hello"
            output: string
        }
    };

    assert_completion_contains!(&completion_suggestions, "openai", "anthropic");

    assert_completion_excludes_labels!(
        &completion_suggestions,
        BuiltinFunctionName::Context,
        ReferenceKeyword::Agent,
        AgentExpressionPropertyName::Prompt,
        DeclarationKeyword::Provider,
        InferenceSetting::MaxTokens,
        "string"
    );

    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Keyword);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Module);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Property);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Variable);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Type);
    assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Value);

    let openai_completion = completion_suggestion_by_label(&completion_suggestions, "openai");

    assert_eq!(openai_completion.insert_text, "openai(\"$1\")");
}

#[test]
fn suggests_reference_roots_inside_model_call_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini", "gpt-4o-mini"]
        }

        agent writer {
            model: openai(<cursor>)
            prompt: "hello"
            output: string
        }
    };

    assert_completion_contains!(&completion_suggestions, "gpt-4.1-mini", "gpt-4o-mini");
    assert_completion_contains_labels!(
        &completion_suggestions,
        ReferenceKeyword::Agent,
        ReferenceKeyword::Input,
        ReferenceKeyword::Secrets
    );

    let gpt_model_completion = completion_suggestion_by_label(&completion_suggestions, "gpt-4.1-mini");

    assert_eq!(gpt_model_completion.insert_text, "\"gpt-4.1-mini\"");
}

#[test]
fn completion_text_edit_range_replaces_model_provider_prefix() {
    let source = inline_document_template! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini", "gpt-4o-mini"]
        }

        agent writer {
            model: op
            prompt: "hello"
            output: string
        }
    }
    .to_string();

    let model_line_index = source
        .lines()
        .position(|source_line| source_line.contains("model: op"))
        .expect("model line should exist in source");

    let model_line = source
        .lines()
        .nth(model_line_index)
        .expect("model line should exist by discovered line index");

    let provider_prefix_character_index = model_line
        .find("op")
        .map(|provider_prefix_start| provider_prefix_start + "op".chars().count())
        .expect("model line should contain provider prefix");

    let cursor_position = Position {
        line: u32::try_from(model_line_index).expect("model line index should fit in u32"),
        character: u32::try_from(provider_prefix_character_index).expect("provider prefix character index should fit in u32"),
    };

    let document_state = DocumentState::new(source);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("model property completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character - 2);
}

#[test]
fn completion_text_edit_range_inserts_model_name_at_empty_string_cursor() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini", "gpt-4o-mini"]
        }

        agent writer {
            model: openai("<cursor>")
            prompt: "hello"
            output: string
        }
    });

    let document_state = DocumentState::new(source);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("model name completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_prompt_value_keeps_space_after_separator() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            prompt: <cursor>
            output: string
        }
    });

    let document_state = DocumentState::new(source);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("prompt completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
}

#[test]
fn completion_text_edit_range_for_prompt_reference_replaces_only_reference_prefix() {
    let (source, cursor_position) = source_with_cursor(inline_document_template! {
        agent writer {
            prompt: agent.<cursor>
            output: string
        }
    });

    let document_state = DocumentState::new(source);
    let completion_text_edit_range = document_state
        .completion_text_edit_range(cursor_position)
        .expect("prompt reference completion should include a replacement range");

    assert_eq!(completion_text_edit_range.start.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.start.character, cursor_position.character - 6);
    assert_eq!(completion_text_edit_range.end.line, cursor_position.line);
    assert_eq!(completion_text_edit_range.end.character, cursor_position.character);
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
fn suppresses_field_completion_after_dot_access_on_nullable_reference_path() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            output: {
                nested: {
                    value: string
                } | null
            }
        }

        output {
            greeting: agent.greeting.nested.<cursor>
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suggests_field_completion_after_optional_access_on_nullable_reference_path() {
    let completion_suggestions = inline_completion_suggestions! {
        agent greeting {
            output: {
                nested: {
                    value: string
                } | null
            }
        }

        output {
            greeting: agent.greeting.nested?.<cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "value");
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
fn uses_schema_field_description_in_reference_completion_documentation() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            first_name: string "first name from schema"
            age: number
        }

        input {
            profile: schema.Person
        }

        agent writer {
            prompt: "hello {{ input.profile.<cursor> }}"
            output: string
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
fn suppresses_key_suggestions_inside_agent_output_object_literal() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output: {
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
            output: {
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
            output: <cursor>
        }
    };

    assert_completion_contains!(&completion_suggestions, "[string]");
}

#[test]
fn suppresses_suggestions_inside_agent_output_type_description_string() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output: boolean "<cursor>"
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_agent_property_suggestions_inside_agent_output_object_field_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output: {
                property: <cursor>
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Tools,
        AgentExpressionPropertyName::Prompt
    );
}

#[test]
fn suggests_only_types_inside_nested_agent_output_object_field_value() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output: {
                property: {
                    id: <cursor>
                }
            }
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Tools,
        DeclarationKeyword::Provider
    );
}

#[test]
fn suppresses_suggestions_inside_agent_output_array_fixed_length_slot() {
    let completion_suggestions = inline_completion_suggestions! {
        agent findings {
            output: {
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
