use super::*;

#[test]
fn reports_unknown_agent_property_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent writer {
            model: model.openai_model
            instruction: "hello"
            retries: 3
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownAgentProperty);
}

#[test]
fn reports_invalid_inference_setting_value_type_diagnostic() {
    let diagnostics = inline_diagnostics! {
        model fast from openai {
            id: "gpt-4.1-mini"

            inference {
                temperature: 0.2
                max_tokens: "2_000"
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidInferenceSettingValueType);
}

#[test]
fn reports_direct_agent_inference_as_unknown_property() {
    let diagnostics = inline_diagnostics! {
        agent writer {
            inference {
                temperature: 0.2
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownAgentProperty);
}

#[test]
fn reports_duplicate_property_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            model: model.ollama_model
            instruction: "Write a short welcome message."
            instruction: "Write a short welcome message."
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::DuplicateProperty);
}
