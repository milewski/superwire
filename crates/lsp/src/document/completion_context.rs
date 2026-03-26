use super::text_utils::trailing_identifier;
use engine_ai_core::dsl::{AgentExpressionPropertyName, DeclarationKeyword};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeclarationHeaderCompletionContext {
    NamedDeclaration,
    SingletonDeclaration,
}

impl DeclarationHeaderCompletionContext {
    pub(super) fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();

        if trimmed_line_prefix.contains(':') || trimmed_line_prefix.contains('{') {
            return None;
        }

        let declaration_keyword = Self::declaration_keyword_from_prefix(trimmed_line_prefix)?;
        let line_after_keyword = trimmed_line_prefix.strip_prefix(declaration_keyword.as_str())?;

        if !line_after_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        let trimmed_line_after_keyword = line_after_keyword.trim_start();

        match declaration_keyword {
            DeclarationKeyword::Provider | DeclarationKeyword::Schema | DeclarationKeyword::Agent => {
                if trimmed_line_after_keyword.is_empty() {
                    return Some(Self::NamedDeclaration);
                }

                if !trimmed_line_after_keyword.contains(char::is_whitespace) {
                    return Some(Self::NamedDeclaration);
                }

                None
            }
            DeclarationKeyword::Input | DeclarationKeyword::Secrets | DeclarationKeyword::Output => {
                if trimmed_line_after_keyword.starts_with('{') {
                    return None;
                }

                Some(Self::SingletonDeclaration)
            }
        }
    }

    fn declaration_keyword_from_prefix(trimmed_line_prefix: &str) -> Option<DeclarationKeyword> {
        let declaration_keyword_identifier = trimmed_line_prefix.split_whitespace().next().unwrap_or(trimmed_line_prefix);

        DeclarationKeyword::from_identifier(declaration_keyword_identifier)
    }
}

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
pub(super) struct AgentPropertyValueCompletionContext {
    pub(super) property_name: AgentExpressionPropertyName,
    pub(super) value_prefix: String,
}

impl AgentPropertyValueCompletionContext {
    pub(super) fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;
        let property_name = AgentExpressionPropertyName::from_identifier(property_name_identifier)?;

        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        Some(Self {
            property_name,
            value_prefix: value_completion_context.value_prefix,
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
