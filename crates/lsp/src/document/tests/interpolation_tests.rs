use super::*;

#[test]
fn completes_agent_references_inside_prompt_string_interpolation() {
    let (source, cursor_position) = inline_document_with_cursor! {
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
    };

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains!(&completion_suggestions, "context_agent");
    assert_completion_excludes_labels!(&completion_suggestions, "worker");
}

#[test]
fn completes_agent_references_inside_multiline_prompt_string_interpolation() {
    let (source, cursor_position) = source_with_cursor(
        r#"
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
                prompt: """
                    example {{ agent.<cursor> }}
                """
                output: string
            }
            "#,
    );

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert_completion_contains!(&completion_suggestions, "context_agent");
}

#[test]
fn suppresses_suggestions_inside_plain_multiline_prompt_string_text() {
    let (source, cursor_position) = source_with_cursor(
        r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: """
                    Like this <cursor>
                """
                output: string
            }
            "#,
    );

    let document_state = DocumentState::new(source);
    let completion_suggestions = document_state.completion_suggestions(cursor_position);

    assert!(completion_suggestions.is_empty());
}
