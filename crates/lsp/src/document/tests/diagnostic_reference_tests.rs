use super::*;

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
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        schema payload {
            value: string
        }

        input {
            query: string
        }

        secrets {
            api_key: string
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
            instruction: "example {{ agent.context_agent }} {{ input.query }} {{ schema.payload }} {{ secrets.api_key }}"
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}

#[test]
fn reports_secret_reference_in_multiline_prompt_string_interpolation_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        input {
            query: string
        }

        secrets {
            api_key: string
        }

        agent worker {
            model: model.openai_model
            instruction: """
                example {{ input.query }}
                forbidden {{ secrets.api_key }}
            """
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}

#[test]
fn reports_missing_optional_reference_access_diagnostic_for_nullable_path() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            output {
                nested: maybe {
                    value: string
                }
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
            output {
                tasks: [{ id: number }]
                participants: [{ id: number }]
            }
        }

        agent analyzer for participant in agent.summarizer {
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidForLoopIterableType);
}

#[test]
fn reports_invalid_reference_path_for_for_loop_agent_output_field() {
    let diagnostics = inline_diagnostics! {
        agent random for number in [1, 2, 3] {
            instruction: "Give me a random user name and age"
            output {
                user: (string, number)
            }
        }

        agent surname {
            instruction: "Give a surname to this user {{ agent.random.user }}"
            output {
                surname: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidReferencePath);
}

#[test]
fn reports_unknown_local_binding_for_missing_for_loop_template_binding() {
    let diagnostics = inline_diagnostics! {
        dynamic {
            examples: [
                {
                    id: 123
                    text: "example"
                },
            ]
        }

        agent stage_1 for { id, text } in dynamic.examples {
            instruction: "transcript: {{ answer }}"
            output {
                summary: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownLocalBindingReference);
}
