use super::*;

#[test]
fn reports_parse_diagnostics_for_invalid_syntax() {
    let document_state = DocumentState::new("agent broken {\n    prompt: \"hello\"\n".to_string());
    let diagnostics = document_state.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::ParseError);
}

#[test]
fn reports_unknown_model_for_provider_diagnostic() {
    let (source, _cursor_position) = inline_document_with_cursor! {
        provider openai {
            driver: "openai"
            models: ["gpt-4.1-mini"]
        }

        agent writer {
            model: openai("gpt-4.1")
            prompt: "hello"
            output: string
        }
        <cursor>
    };

    let document_state = DocumentState::new(source);
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownModelForProvider);
}

#[test]
fn reports_unknown_agent_property_diagnostic() {
    let (source, _cursor_position) = inline_document_with_cursor! {
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
        <cursor>
    };

    let document_state = DocumentState::new(source);
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownAgentProperty);
}

#[test]
fn reports_invalid_bare_tool_reference_diagnostic() {
    let (source, _cursor_position) = inline_document_with_cursor! {
        agent tooling {
            tools: [tool]
        }

        <cursor>
    };

    let document_state = DocumentState::new(source);
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidKeywordReferenceRoot);
}

#[test]
fn reports_secret_reference_in_prompt_string_interpolation_diagnostic() {
    let (source, _cursor_position) = inline_document_with_cursor! {
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

        <cursor>
    };

    let document_state = DocumentState::new(source);
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}

#[test]
fn reports_secret_reference_in_multiline_prompt_string_interpolation_diagnostic() {
    let source = r#"
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
        "#;

    let document_state = DocumentState::new(source.to_string());
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
}
