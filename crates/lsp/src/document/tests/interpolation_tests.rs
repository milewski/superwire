use super::*;

#[test]
fn completes_agent_references_inside_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
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

    assert_completion_contains!(&completion_suggestions, "context_agent");
    assert_completion_excludes_labels!(&completion_suggestions, "worker");
}

#[test]
fn suggests_only_agent_and_input_roots_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        input {
            customer_name: string
        }

        agent writer {
            prompt: "Write a short welcome message. {{ <cursor> }}"
            output: string
        }
    };

    assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Agent, ReferenceKeyword::Input);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        ReferenceKeyword::Secrets,
        ReferenceKeyword::Tool,
        BuiltinFunctionName::Context,
        BuiltinFunctionName::Template,
        BuiltinFunctionName::Compact,
        DeclarationKeyword::Schema,
        DeclarationKeyword::Provider,
        "string",
        "number"
    );
}

#[test]
fn suppresses_invalid_schema_root_suggestions_inside_interpolation_expression() {
    let completion_suggestions = inline_completion_suggestions! {
        schema Person {
            name: string
        }

        agent writer {
            prompt: "Write a short welcome message. {{ schema.<cursor> }}"
            output: string
        }
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn completes_agent_references_inside_multiline_prompt_string_interpolation() {
    let completion_suggestions = inline_completion_suggestions! {
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
    };

    assert_completion_contains!(&completion_suggestions, "context_agent");
}

#[test]
fn suppresses_suggestions_inside_plain_multiline_prompt_string_text() {
    let completion_suggestions = inline_completion_suggestions! {
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
    };

    assert!(completion_suggestions.is_empty());
}

#[test]
fn suppresses_suggestions_inside_plain_single_line_prompt_string_text() {
    let source = [
        "provider openai {",
        "    driver: \"openai\"",
        "    models: [\"gpt-4.1-mini\"]",
        "}",
        "",
        "agent worker {",
        "    model: openai(\"gpt-4.1-mini\")",
        "    prompt: \" a short note for input number {{ n }}\"",
        "    output: string",
        "}",
    ]
    .join("\n");
    let cursor_offset = source
        .find("    prompt: \"")
        .expect("test source should contain prompt string prefix")
        + "    prompt: \"".len();
    let cursor_position = position_from_source_offset(&source, cursor_offset);
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert!(completion_suggestions.is_empty());
}

fn position_from_source_offset(source_text: &str, source_offset: usize) -> Position {
    let mut line = 0_u32;
    let mut character = 0_u32;

    for character_in_source in source_text[..source_offset].chars() {
        if character_in_source == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    Position { line, character }
}
