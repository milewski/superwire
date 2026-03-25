use super::text_utils::trailing_identifier;

#[derive(Debug, Clone)]
pub(super) struct ModelCallCompletionContext {
    pub(super) provider_name: String,
    pub(super) model_prefix: String,
    pub(super) inside_string_literal: bool,
}

impl ModelCallCompletionContext {
    pub(super) fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_prefix = line_prefix.trim_end();
        let open_parenthesis_index = trimmed_prefix.rfind('(')?;

        let callee_prefix = trimmed_prefix[..open_parenthesis_index].trim_end();
        let provider_name = trailing_identifier(callee_prefix)?.to_string();
        let argument_prefix = &trimmed_prefix[open_parenthesis_index + 1..];

        if argument_prefix.contains(')') {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(argument_prefix);

        Some(Self {
            provider_name,
            model_prefix: value_completion_context.value_prefix,
            inside_string_literal: value_completion_context.inside_string_literal,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct ValueCompletionContext {
    pub(super) value_prefix: String,
    pub(super) inside_string_literal: bool,
}

impl ValueCompletionContext {
    pub(super) fn from_value_prefix(value_prefix: &str) -> Self {
        let trimmed_value_prefix = value_prefix.trim_start();
        let quotation_count = trimmed_value_prefix.chars().filter(|character| *character == '"').count();

        if quotation_count % 2 == 1 {
            let last_quote_index = trimmed_value_prefix.rfind('"').unwrap_or(0);

            return Self {
                value_prefix: trimmed_value_prefix[last_quote_index + 1..].to_string(),
                inside_string_literal: true,
            };
        }

        Self {
            value_prefix: trimmed_value_prefix.to_string(),
            inside_string_literal: false,
        }
    }
}
