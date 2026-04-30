use super::*;

#[test]
fn reports_parse_diagnostics_for_invalid_syntax() {
    let diagnostics = inline_diagnostics! {
        agent broken {
            prompt
        } @
    };

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::ParseError);
}

#[test]
fn reports_unknown_model_for_provider_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent writer {
            model: openai("gpt-4.1")
            prompt: "hello"
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownModelForProvider);
}

#[test]
fn allows_dynamic_model_reference_without_literal_model_diagnostic() {
    let diagnostics = inline_diagnostics! {
        secrets {
            openai_model: string
        }

        provider openai {
            driver: "openai"
            models: [secrets.openai_model]
        }

        agent writer {
            model: openai(secrets.openai_model)
            prompt: "hello"
            output: string
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidModelExpression));
    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::UnknownModelForProvider));
}

#[test]
fn reports_parse_error_for_unknown_agent_property() {
    let diagnostics = inline_diagnostics! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent writer {
            model: openai("gpt-4.1-mini")
            prompt: "hello"
            retries: 3
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::ParseError);
}

#[test]
fn reports_invalid_inference_setting_value_type_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent writer {
            inference: {
                temperature: 0.2
                max_tokens: "2_000"
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidInferenceSettingValueType);
}

#[test]
fn reports_invalid_bare_tool_reference_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent tooling {
            tools: [tool]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidKeywordReferenceRoot);
}

#[test]
fn reports_parse_error_for_call_style_tool_binding_overrides() {
    let diagnostics = inline_diagnostics! {
        agent tooling {
            tools: [
                tool.fetch_participant_answer(project_id: input.project_id, task_id: input.task_id)
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::ParseError);
}

#[test]
fn allows_block_style_tool_binding_overrides() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: string
            task_id: string
        }

        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        tool fetch_participant_answer {
            bindings {
                project_id: string
                task_id: string
            }
        }

        agent participant_answer_analyzer {
            model: openai("gpt-4.1-mini")
            tools: [
                tool.fetch_participant_answer {
                    bindings {
                        project_id: input.project_id
                        task_id: input.task_id
                    }
                }
            ]
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::ParseError));
    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::UnknownToolReference));
}

#[test]
fn reports_missing_tool_binding_overrides_diagnostic() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: number
            task_id: number
        }

        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        dynamic {
            data: {
                participants: []
            }
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: number
                task_id: number
            }
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: openai("gpt-4.1-mini")
            tools: [
                tool.fetch_participant_answer
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
}

#[test]
fn allows_fixed_tool_bindings_without_overrides() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: number
        }

        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        dynamic {
            data: {
                participants: []
            }
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: input.project_id
                task_id: 123
            }
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: openai("gpt-4.1-mini")
            tools: [
                tool.fetch_participant_answer
            ]
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidToolBinding));
}

#[test]
fn reports_invalid_tool_binding_override_type_diagnostic() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: string
            task_id: number
        }

        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: number
                task_id: number
            }
        }

        agent participant_answer_analyzer {
            model: openai("gpt-4.1-mini")
            tools: [
                tool.fetch_participant_answer {
                    bindings {
                        project_id: input.project_id
                        task_id: input.task_id
                    }
                }
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
}

#[test]
fn allows_blockless_deterministic_tool_calls() {
    let diagnostics = inline_diagnostics! {
        tool list_participants {
            output {
                count: number
            }
        }

        dynamic {
            data: call tool.list_participants
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::ParseError));
}

#[test]
fn reports_dynamic_dependency_cycle_diagnostic() {
    let diagnostics = inline_diagnostics! {
        dynamic {
            a: dynamic.b
        }

        dynamic {
            b: dynamic.a
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::DynamicDependencyCycle);
}

#[test]
fn allows_dynamic_references_to_later_dynamic_blocks() {
    let diagnostics = inline_diagnostics! {
        dynamic {
            a: dynamic.max_results
        }

        dynamic {
            max_results: 5
            timeout_seconds: 30
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::UnknownDynamicFieldReference));
    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::DynamicDependencyCycle));
}

#[test]
fn reports_secret_reference_in_prompt_string_interpolation_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        schema Payload {
            value: string
        }

        input {
            query: string
        }

        secrets {
            api_key: string
        }

        agent context_agent {
            model: openai("gpt-4.1-mini")
            prompt: "hello"
            output: string
        }

        agent worker {
            model: openai("gpt-4.1-mini")
            prompt: "example {{ agent.context_agent }} {{ input.query }} {{ schema.Payload }} {{ secrets.api_key }}"
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}

#[test]
fn reports_secret_reference_in_multiline_prompt_string_interpolation_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        input {
            query: string
        }

        secrets {
            api_key: string
        }

        agent worker {
            model: openai("gpt-4.1-mini")
            prompt: """
                example {{ input.query }}
                forbidden {{ secrets.api_key }}
            """
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}

#[test]
fn reports_missing_optional_reference_access_diagnostic_for_nullable_path() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            output: {
                nested: {
                    value: string
                } | null
            }
        }

        output {
            greeting: agent.greeting.nested.value
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::MissingOptionalReferenceAccess);
}

#[test]
fn reports_invalid_for_loop_iterable_type_diagnostic_for_object_reference() {
    let diagnostics = inline_diagnostics! {
        agent summarizer {
            output: {
                tasks: [{ id: number }]
                participants: [{ id: number }]
            }
        }

        agent analyzer for participant in agent.summarizer {
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidForLoopIterableType);
}

#[test]
fn reports_duplicate_property_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            model: ollama("qwen3.5:8b")
            prompt: "Write a short welcome message."
            prompt: "Write a short welcome message."
            output: string
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::DuplicateProperty);
}

#[test]
fn reports_invalid_type_expression_reference_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            output: test
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidTypeExpressionReference);
}

#[test]
fn accepts_nested_schema_enum_field_reference_diagnostics() {
    let diagnostics = inline_diagnostics! {
        schema main {
            language: "en_US" | "zh_CN" | "fr"
        }

        input {
            workspace_id: string
            scope: string
        }

        tool create_project_for_workspace {
            description: "Create a new project in the bound workspace and scope."
            input {
                name: [{
                    language: schema.main.language
                    value: string "localized project name"
                }]
                primary_language: schema.main.language "primary locale language code"
                languages: [schema.main.language] "supported locale language code"
            }
            bindings {
                workspace_id: input.workspace_id
                scope: input.scope
            }
        }
    };

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidTypeExpressionReference),
        "unexpected invalid type reference diagnostics: {diagnostics:?}"
    );
}
