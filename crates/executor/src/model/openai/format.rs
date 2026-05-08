pub(super) fn format_tool_name(tool_name: &str) -> String {
    format_openai_identifier(tool_name, "tool")
}

fn format_openai_identifier(identifier: &str, fallback: &str) -> String {
    let mut formatted_identifier = identifier
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    if formatted_identifier.is_empty() {
        formatted_identifier = fallback.to_string();
    }

    formatted_identifier.truncate(64);
    formatted_identifier
}
