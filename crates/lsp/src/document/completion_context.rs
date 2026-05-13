use super::text_utils::{
    for_clause_iterable_prefix, is_identifier, leading_identifier, split_for_clause_binding, trailing_identifier, trailing_reference_token,
};
use super::{CompletionKind, CompletionSuggestion};
use superwire_core::dsl::{AgentExpressionPropertyName, DeclarationKeyword, ForClauseKeyword, ReferenceKeyword};
use superwire_core::semantic::InferenceSetting;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationHeaderCompletionContext {
    NamedDeclaration,
    SingletonDeclaration,
    NamedDeclarationBlock,
    AgentForKeyword { keyword_prefix: String },
    AgentForIteratorName,
    AgentInKeyword { keyword_prefix: String },
}

impl DeclarationHeaderCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();

        let mut declaration_line_prefix = trimmed_line_prefix;
        let trimmed_line_prefix_without_trailing_whitespace = trimmed_line_prefix.trim_end();

        if let Some(prefix_before_trailing_brace) = trimmed_line_prefix_without_trailing_whitespace.strip_suffix('{') {
            declaration_line_prefix = prefix_before_trailing_brace.trim_end();
        }

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
            DeclarationKeyword::Provider
            | DeclarationKeyword::Model
            | DeclarationKeyword::Mcp
            | DeclarationKeyword::Schema
            | DeclarationKeyword::Tool
            | DeclarationKeyword::Resource
            | DeclarationKeyword::Prompt
            | DeclarationKeyword::Agent => {
                if declaration_keyword == DeclarationKeyword::Agent {
                    return Self::agent_header_completion_context(trimmed_line_after_keyword);
                }

                if trimmed_line_after_keyword.is_empty() || !trimmed_line_after_keyword.contains(char::is_whitespace) {
                    return Some(Self::NamedDeclaration);
                }

                if trimmed_line_after_keyword.split_whitespace().count() == 1 {
                    return Some(Self::NamedDeclarationBlock);
                }

                None
            }
            DeclarationKeyword::Dynamic | DeclarationKeyword::Input | DeclarationKeyword::Secrets | DeclarationKeyword::Output => {
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
            Self::NamedDeclarationBlock => vec![CompletionSuggestion {
                label: "{}".to_string(),
                kind: CompletionKind::Value,
                detail: "Declaration block".to_string(),
                documentation: "Insert declaration block braces.".to_string(),
                insert_text: "{}".to_string(),
            }],
            Self::AgentForKeyword { keyword_prefix } => Self::for_clause_keyword_suggestions(ForClauseKeyword::For, &keyword_prefix),
            Self::AgentInKeyword { keyword_prefix } => Self::for_clause_keyword_suggestions(ForClauseKeyword::In, &keyword_prefix),
        }
    }

    fn agent_header_completion_context(trimmed_line_after_keyword: &str) -> Option<Self> {
        if trimmed_line_after_keyword.is_empty() {
            return Some(Self::NamedDeclaration);
        }

        let line_has_trailing_whitespace = trimmed_line_after_keyword.ends_with(char::is_whitespace);
        let agent_name = leading_identifier(trimmed_line_after_keyword)?;
        let after_agent_name = &trimmed_line_after_keyword[agent_name.len()..];

        if after_agent_name.trim().is_empty() {
            if line_has_trailing_whitespace {
                return Some(Self::AgentForKeyword {
                    keyword_prefix: String::new(),
                });
            }

            return Some(Self::NamedDeclaration);
        }

        let after_agent_name = after_agent_name.trim_start();
        let for_keyword_segment = leading_identifier(after_agent_name).unwrap_or_default();

        if for_keyword_segment.is_empty() {
            return None;
        }

        if ForClauseKeyword::from_identifier(for_keyword_segment) != Some(ForClauseKeyword::For) {
            return Some(Self::AgentForKeyword {
                keyword_prefix: for_keyword_segment.to_string(),
            });
        }

        let after_for_keyword = &after_agent_name[for_keyword_segment.len()..];

        if after_for_keyword.trim_start().is_empty() {
            return Some(Self::AgentForIteratorName);
        }

        let (for_binding_text, after_for_binding) = split_for_clause_binding(after_for_keyword)?;

        if after_for_binding.is_empty() && !line_has_trailing_whitespace {
            if for_binding_text.starts_with('{') {
                return Some(Self::AgentInKeyword {
                    keyword_prefix: String::new(),
                });
            }

            return Some(Self::AgentForIteratorName);
        }

        let after_for_binding = after_for_binding.trim_start();

        if after_for_binding.is_empty() {
            return Some(Self::AgentInKeyword {
                keyword_prefix: String::new(),
            });
        }

        let in_keyword_segment = leading_identifier(after_for_binding).unwrap_or_default();

        if in_keyword_segment.is_empty() {
            return None;
        }

        if line_has_trailing_whitespace {
            if ForClauseKeyword::from_identifier(in_keyword_segment) == Some(ForClauseKeyword::In) {
                return None;
            }

            return Some(Self::AgentInKeyword {
                keyword_prefix: in_keyword_segment.to_string(),
            });
        }

        if ForClauseKeyword::from_identifier(in_keyword_segment) == Some(ForClauseKeyword::In) {
            return None;
        }

        Some(Self::AgentInKeyword {
            keyword_prefix: in_keyword_segment.to_string(),
        })
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
    pub replaces_empty_string_literal: bool,
}

#[derive(Debug, Clone)]
pub struct ToolCallCompletionContext {
    pub tool_name: String,
    pub argument_prefix: String,
    pub existing_argument_names: Vec<String>,
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
            replaces_empty_string_literal: value_completion_context.inside_string_literal
                && value_completion_context.value_prefix.is_empty(),
            model_prefix: value_completion_context.value_prefix,
            inside_string_literal: value_completion_context.inside_string_literal,
        })
    }
}

impl ToolCallCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_prefix = line_prefix.trim_end();
        let open_parenthesis_index = trimmed_prefix.rfind('(')?;
        let callee_prefix = trimmed_prefix[..open_parenthesis_index].trim_end();
        let callee_token = trailing_reference_token(callee_prefix)?;
        let tool_namespace_prefix = format!("{}.", ReferenceKeyword::Tool.as_str());
        let tool_name = callee_token.strip_prefix(tool_namespace_prefix.as_str())?;

        if !is_identifier(tool_name) {
            return None;
        }

        let arguments_prefix = &trimmed_prefix[open_parenthesis_index + 1..];

        if arguments_prefix.contains(')') {
            return None;
        }

        let current_argument_prefix = arguments_prefix
            .rsplit_once(',')
            .map_or(arguments_prefix, |(_, after_comma)| after_comma);

        if current_argument_prefix.contains(':') {
            return None;
        }

        Some(Self {
            tool_name: tool_name.to_string(),
            argument_prefix: trailing_identifier(current_argument_prefix).unwrap_or_default().to_string(),
            existing_argument_names: parse_existing_tool_argument_names(arguments_prefix),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentPropertyValueCompletionContext {
    pub property_name: AgentExpressionPropertyName,
    pub value_prefix: String,
    pub inside_string_literal: bool,
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
            inside_string_literal: value_completion_context.inside_string_literal,
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

#[derive(Debug, Clone)]
pub struct ForLoopDestructuringBindingCompletionContext {
    pub field_prefix: String,
    pub existing_field_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OutputValueCompletionContext {
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
        Some(Self {
            value_prefix: for_clause_iterable_prefix(line_prefix)?,
        })
    }
}

impl ForLoopDestructuringBindingCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let for_clause_separator = format!(" {} ", ForClauseKeyword::For.as_str());
        let (_, after_for_clause_separator) = trimmed_line_prefix.rsplit_once(for_clause_separator.as_str())?;
        let after_for_clause_separator = after_for_clause_separator.trim_start();
        let after_opening_brace = after_for_clause_separator.strip_prefix('{')?;

        if after_opening_brace.contains('}') {
            return None;
        }

        let field_prefix = trailing_identifier(after_opening_brace).unwrap_or_default().to_string();
        let mut existing_field_names = parse_existing_destructuring_field_names(after_opening_brace);

        if !field_prefix.is_empty() && existing_field_names.last().is_some_and(|field_name| field_name == &field_prefix) {
            let _ = existing_field_names.pop();
        }

        Some(Self {
            field_prefix,
            existing_field_names,
        })
    }
}

fn parse_existing_destructuring_field_names(destructuring_prefix: &str) -> Vec<String> {
    let mut field_names = Vec::new();

    for field_segment in destructuring_prefix.split(',') {
        let candidate_field_name = field_segment.trim();

        if candidate_field_name.is_empty() {
            continue;
        }

        if !is_identifier(candidate_field_name) {
            continue;
        }

        field_names.push(candidate_field_name.to_string());
    }

    field_names
}

fn parse_existing_tool_argument_names(arguments_prefix: &str) -> Vec<String> {
    arguments_prefix
        .split(',')
        .filter_map(|argument_segment| {
            let (argument_name_segment, _) = argument_segment.split_once(':')?;
            let argument_name = trailing_identifier(argument_name_segment.trim_end())?;

            Some(argument_name.to_string())
        })
        .collect()
}

impl OutputValueCompletionContext {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (_, value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        Some(Self {
            value_prefix: value_completion_context.value_prefix,
        })
    }
}
