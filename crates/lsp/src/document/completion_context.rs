use super::text_utils::{leading_identifier, trailing_identifier};
use super::{CompletionKind, CompletionSuggestion};
use engine_ai_core::dsl::{AgentExpressionPropertyName, DeclarationKeyword, ForClauseKeyword};
use engine_ai_core::runtime::InferenceSetting;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationHeaderCompletionContext {
    NamedDeclaration,
    SingletonDeclaration,
    AgentForKeyword { keyword_prefix: String },
    AgentForIteratorName,
    AgentInKeyword { keyword_prefix: String },
}

impl DeclarationHeaderCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();

        let declaration_line_prefix = if let Some((prefix_before_brace, suffix_after_brace)) = trimmed_line_prefix.split_once('{') {
            if !suffix_after_brace.trim().is_empty() {
                return None;
            }

            prefix_before_brace.trim_end()
        } else {
            trimmed_line_prefix
        };

        if declaration_line_prefix.contains(':') {
            return None;
        }

        let declaration_keyword = Self::declaration_keyword_from_prefix(declaration_line_prefix)?;
        let line_after_keyword = declaration_line_prefix.strip_prefix(declaration_keyword.as_str())?;

        if !line_after_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        let trimmed_line_after_keyword = line_after_keyword.trim_start();

        match declaration_keyword {
            DeclarationKeyword::Provider | DeclarationKeyword::Schema | DeclarationKeyword::Agent => {
                if declaration_keyword == DeclarationKeyword::Agent {
                    return Self::agent_header_completion_context(trimmed_line_after_keyword);
                }

                if trimmed_line_after_keyword.is_empty() || !trimmed_line_after_keyword.contains(char::is_whitespace) {
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

    pub fn completion_suggestions(self) -> Vec<CompletionSuggestion> {
        match self {
            Self::NamedDeclaration | Self::SingletonDeclaration | Self::AgentForIteratorName => Vec::new(),
            Self::AgentForKeyword { keyword_prefix } => Self::for_clause_keyword_suggestions(ForClauseKeyword::For, &keyword_prefix),
            Self::AgentInKeyword { keyword_prefix } => Self::for_clause_keyword_suggestions(ForClauseKeyword::In, &keyword_prefix),
        }
    }

    fn agent_header_completion_context(trimmed_line_after_keyword: &str) -> Option<Self> {
        if trimmed_line_after_keyword.is_empty() {
            return Some(Self::NamedDeclaration);
        }

        let mut declaration_segments = trimmed_line_after_keyword.split_whitespace().collect::<Vec<_>>();
        let line_has_trailing_whitespace = trimmed_line_after_keyword.ends_with(char::is_whitespace);

        if declaration_segments.is_empty() {
            return Some(Self::NamedDeclaration);
        }

        if declaration_segments.len() == 1 {
            if line_has_trailing_whitespace {
                return Some(Self::AgentForKeyword {
                    keyword_prefix: String::new(),
                });
            }

            return Some(Self::NamedDeclaration);
        }

        let for_clause_segment = declaration_segments[1];

        if declaration_segments.len() == 2 {
            if ForClauseKeyword::from_identifier(for_clause_segment) == Some(ForClauseKeyword::For) {
                if line_has_trailing_whitespace {
                    return Some(Self::AgentForIteratorName);
                }

                return None;
            }

            return Some(Self::AgentForKeyword {
                keyword_prefix: for_clause_segment.to_string(),
            });
        }

        if ForClauseKeyword::from_identifier(for_clause_segment) != Some(ForClauseKeyword::For) {
            return None;
        }

        if declaration_segments.len() == 3 {
            if line_has_trailing_whitespace {
                return Some(Self::AgentInKeyword {
                    keyword_prefix: String::new(),
                });
            }

            return Some(Self::AgentForIteratorName);
        }

        if declaration_segments.len() == 4 {
            let in_clause_segment = declaration_segments.pop().unwrap_or_default();

            if line_has_trailing_whitespace {
                if ForClauseKeyword::from_identifier(in_clause_segment) == Some(ForClauseKeyword::In) {
                    return None;
                }

                return Some(Self::AgentInKeyword {
                    keyword_prefix: in_clause_segment.to_string(),
                });
            }

            if ForClauseKeyword::from_identifier(in_clause_segment).is_some() {
                return None;
            }

            return Some(Self::AgentInKeyword {
                keyword_prefix: in_clause_segment.to_string(),
            });
        }

        None
    }

    fn for_clause_keyword_suggestions(for_clause_keyword: ForClauseKeyword, keyword_prefix: &str) -> Vec<CompletionSuggestion> {
        if !for_clause_keyword.as_str().starts_with(keyword_prefix) {
            return Vec::new();
        }

        vec![CompletionSuggestion {
            label: for_clause_keyword.as_str().to_string(),
            kind: CompletionKind::Keyword,
            detail: "For-clause keyword".to_string(),
            documentation: "Agent for-loop declaration keyword.".to_string(),
            insert_text: for_clause_keyword.as_str().to_string(),
        }]
    }

    fn declaration_keyword_from_prefix(trimmed_line_prefix: &str) -> Option<DeclarationKeyword> {
        let declaration_keyword_identifier = trimmed_line_prefix.split_whitespace().next().unwrap_or(trimmed_line_prefix);

        DeclarationKeyword::from_identifier(declaration_keyword_identifier)
    }
}

#[derive(Debug, Clone)]
pub struct ModelCallCompletionContext {
    pub provider_name: String,
    pub model_prefix: String,
    pub inside_string_literal: bool,
}

impl ModelCallCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
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
pub struct AgentPropertyValueCompletionContext {
    pub property_name: AgentExpressionPropertyName,
    pub value_prefix: String,
}

impl AgentPropertyValueCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
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
pub struct ValueCompletionContext {
    pub value_prefix: String,
    pub inside_string_literal: bool,
}

#[derive(Debug, Clone)]
pub struct InferenceSettingValueCompletionContext {
    pub inference_setting: InferenceSetting,
    pub value_prefix: String,
    pub inside_string_literal: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ArrayFixedLengthCompletionContext;

#[derive(Debug, Clone)]
pub struct ForLoopIterableValueCompletionContext {
    pub value_prefix: String,
}

impl InferenceSettingValueCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let inference_setting_identifier = trailing_identifier(line_before_value)?;
        let inference_setting = InferenceSetting::from_identifier(inference_setting_identifier)?;
        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        Some(Self {
            inference_setting,
            value_prefix: value_completion_context.value_prefix,
            inside_string_literal: value_completion_context.inside_string_literal,
        })
    }
}

impl ValueCompletionContext {
    pub fn from_value_prefix(value_prefix: &str) -> Self {
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

impl ArrayFixedLengthCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_end();
        let mut unmatched_array_open_offsets = Vec::<usize>::new();

        for (character_offset, character) in trimmed_line_prefix.char_indices() {
            if character == '[' {
                unmatched_array_open_offsets.push(character_offset);

                continue;
            }

            if character == ']' {
                let _ = unmatched_array_open_offsets.pop();
            }
        }

        let array_open_offset = *unmatched_array_open_offsets.last()?;
        let array_contents_before_cursor = &trimmed_line_prefix[array_open_offset + 1..];

        let (_, fixed_length_prefix) = array_contents_before_cursor.rsplit_once(';')?;

        if fixed_length_prefix.contains(':') {
            return None;
        }

        Some(Self)
    }
}

impl ForLoopIterableValueCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        if line_prefix.contains(':') {
            return None;
        }

        let trimmed_line_prefix = line_prefix.trim_start();
        let for_clause_separator = format!(" {} ", ForClauseKeyword::For.as_str());
        let (_, after_for_clause_separator) = trimmed_line_prefix.split_once(for_clause_separator.as_str())?;
        let iterator_name = leading_identifier(after_for_clause_separator)?;
        let after_iterator_name = after_for_clause_separator[iterator_name.len()..].trim_start();
        let after_in_keyword = after_iterator_name.strip_prefix(ForClauseKeyword::In.as_str())?;

        if !after_in_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        Some(Self {
            value_prefix: after_in_keyword.trim_start().to_string(),
        })
    }
}
