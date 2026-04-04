use super::*;

#[test]
fn reports_parse_diagnostics_for_invalid_syntax() {
    let diagnostics = inline_diagnostics! {
        @
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
fn reports_unknown_agent_property_diagnostic() {
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

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownAgentProperty);
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
