use super::*;

#[test]
fn reports_unknown_model_for_provider_diagnostic() {
    let diagnostics = inline_diagnostics! {
        provider openai from openai {}

        agent writer {
            model: model.missing_model
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownModelProfile);
}

#[test]
fn allows_dynamic_model_reference_without_literal_model_diagnostic() {
    let diagnostics = inline_diagnostics! {
        secrets {
            openai_model: string
        }

        provider openai from openai {}

        model openai_model from openai {
            id: secrets.openai_model
        }

        agent writer {
            model: model.openai_model
            instruction: "hello"
            output {
                value: string
            }
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidModelExpression));
    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::UnknownModelProfile));
}
