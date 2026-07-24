use lsp_types::Position;
use superwire_dsl::{
    AgentExpressionPropertyName, DeclarationKeyword, ForClauseKeyword, McpCallOperation, ReferenceKeyword, ToolCallKeyword,
};
use superwire_semantic::InferenceSetting;

use super::super::completion_context::{
    AgentPropertyValueCompletionContext, ForLoopIterableValueCompletionContext, InferenceSettingValueCompletionContext,
};
use super::super::reference::{ReferenceCompletionConstraint, ReferenceCompletionPath};
use super::super::scope::CompletionScope;
use super::super::semantic_index::SemanticIndex;
use super::super::text_utils::trailing_reference_token;
use super::super::{CompletionSuggestion, DocumentState};

pub(in crate::document) struct ReferenceCompletionInputs<'completion> {
    pub(super) line_prefix: &'completion str,
    pub(super) line_suffix: &'completion str,
    pub(super) position: Position,
    pub(super) completion_scope: CompletionScope,
    pub(super) inside_interpolation_expression: bool,
    pub(super) inference_setting_value_completion_context: Option<&'completion InferenceSettingValueCompletionContext>,
}

impl DocumentState {
    fn has_existing_tool_binding_block(line_suffix: &str) -> bool {
        matches!(line_suffix.trim_start().chars().next(), Some('{' | '('))
    }

    pub(super) fn reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        inputs: ReferenceCompletionInputs<'_>,
    ) -> Option<Vec<CompletionSuggestion>> {
        let position_context = self.position_context(inputs.position)?;
        let line_has_property_separator = inputs.line_prefix.trim_start().contains(':');
        let reference_completion_path = ReferenceCompletionPath::from_line_prefix(inputs.line_prefix)?;
        let for_loop_iterable_reference_context = matches!(
            reference_completion_path.root_keyword(),
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Secrets)
        ) && Self::is_for_loop_iterable_clause_context(inputs.line_prefix);

        let reference_completion_constraint = if for_loop_iterable_reference_context {
            ReferenceCompletionConstraint::ForLoopIterable
        } else {
            self.reference_completion_constraint(inputs.line_prefix, inputs.inference_setting_value_completion_context)
        };

        let reference_suggestions = semantic_index.reference_path_suggestions(
            &reference_completion_path,
            reference_completion_constraint,
            position_context,
            Self::has_existing_tool_binding_block(inputs.line_suffix),
        );

        if Self::is_mcp_call_callee_context(inputs.line_prefix, &reference_completion_path) {
            return Some(reference_suggestions);
        }

        if let Some(inference_suggestions) = self.inference_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            inputs.inference_setting_value_completion_context,
        ) {
            return Some(inference_suggestions);
        }

        if let Some(interpolation_suggestions) = self.interpolation_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            inputs.position,
            &reference_completion_path,
            &reference_suggestions,
            inputs.inside_interpolation_expression,
        ) {
            return Some(interpolation_suggestions);
        }

        if let Some(output_suggestions) = self.output_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            inputs.position,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
        ) {
            return Some(output_suggestions);
        }

        if let Some(prompt_suggestions) = self.prompt_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
            inputs.inside_interpolation_expression,
        ) {
            return Some(prompt_suggestions);
        }

        if let Some(dynamic_value_suggestions) = Self::dynamic_value_reference_completion_suggestions(
            semantic_index,
            inputs.completion_scope,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
        ) {
            return Some(dynamic_value_suggestions);
        }

        self.default_reference_completion_suggestions(&reference_completion_path, &reference_suggestions, reference_completion_constraint)
    }

    fn inference_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> Option<Vec<CompletionSuggestion>> {
        inference_setting_value_completion_context?;

        let can_suggest_inference_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_inference_roots {
                    return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(
                ReferenceKeyword::Secrets
                | ReferenceKeyword::Model
                | ReferenceKeyword::Tool
                | ReferenceKeyword::Resource
                | ReferenceKeyword::Prompt,
            )
            | None => {
                if can_suggest_inference_roots {
                    return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn interpolation_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !inside_interpolation_expression {
            return None;
        }

        let can_suggest_interpolation_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);
        let prompt_interpolation_reference_context =
            Self::is_prompt_interpolation_reference_context(line_prefix, inside_interpolation_expression);
        let for_loop_iterator_reference_root =
            semantic_index.has_for_loop_binding_at_position(self.position_context(position)?, reference_completion_path.root_identifier());

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_interpolation_roots {
                    return Some(self.interpolation_root_suggestions_for_context(
                        semantic_index,
                        line_prefix,
                        position,
                        reference_completion_path.root_identifier(),
                        inside_interpolation_expression,
                    ));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Secrets) => {
                if prompt_interpolation_reference_context {
                    if can_suggest_interpolation_roots {
                        return Some(self.interpolation_root_suggestions_for_context(
                            semantic_index,
                            line_prefix,
                            position,
                            reference_completion_path.root_identifier(),
                            inside_interpolation_expression,
                        ));
                    }

                    return Some(Vec::new());
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Model | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if for_loop_iterator_reference_root {
                    return Some(reference_suggestions.to_vec());
                }

                if can_suggest_interpolation_roots {
                    return Some(self.interpolation_root_suggestions_for_context(
                        semantic_index,
                        line_prefix,
                        position,
                        reference_completion_path.root_identifier(),
                        inside_interpolation_expression,
                    ));
                }

                Some(Vec::new())
            }
        }
    }

    fn output_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        line_has_property_separator: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !line_has_property_separator || !semantic_index.is_output_position(self.position_context(position)?) {
            return None;
        }

        let can_suggest_output_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Secrets) => {
                if can_suggest_output_roots {
                    return Some(semantic_index.output_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Model | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if can_suggest_output_roots {
                    return Some(semantic_index.output_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn prompt_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        let is_prompt_property_reference = line_has_property_separator
            && !inside_interpolation_expression
            && AgentPropertyValueCompletionContext::from_line_prefix(line_prefix)
                .is_some_and(|completion_context| completion_context.property_name == AgentExpressionPropertyName::Instruction);

        if !is_prompt_property_reference {
            return None;
        }

        let can_suggest_prompt_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_prompt_roots {
                    return Some(semantic_index.prompt_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(
                ReferenceKeyword::Secrets
                | ReferenceKeyword::Model
                | ReferenceKeyword::Tool
                | ReferenceKeyword::Resource
                | ReferenceKeyword::Prompt,
            )
            | None => {
                if can_suggest_prompt_roots {
                    return Some(semantic_index.prompt_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn dynamic_value_reference_completion_suggestions(
        semantic_index: &SemanticIndex,
        completion_scope: CompletionScope,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::DynamicValues || !line_prefix.trim_start().contains(':') {
            return None;
        }

        if Self::can_suggest_reference_roots(line_prefix, reference_completion_path) {
            return Some(semantic_index.dynamic_value_suggestions(reference_completion_path.root_identifier()));
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Input | ReferenceKeyword::Secrets) => {
                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Tool) => {
                if Self::is_tool_call_callee_context(line_prefix, reference_completion_path) {
                    return Some(reference_suggestions.to_vec());
                }

                Some(Vec::new())
            }
            Some(ReferenceKeyword::Model | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) => {
                if Self::is_mcp_call_callee_context(line_prefix, reference_completion_path) {
                    return Some(reference_suggestions.to_vec());
                }

                Some(Vec::new())
            }
            None => Some(Vec::new()),
        }
    }

    fn is_tool_call_callee_context(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        if reference_completion_path.root_keyword() != Some(ReferenceKeyword::Tool) {
            return false;
        }

        let Some(reference_token) = trailing_reference_token(line_prefix) else {
            return false;
        };

        let Some(reference_start_index) = line_prefix.rfind(reference_token) else {
            return false;
        };

        let value_prefix = line_prefix[..reference_start_index].trim_end();
        let Some((_, value_prefix_after_separator)) = value_prefix.rsplit_once(':') else {
            return false;
        };

        value_prefix_after_separator.trim() == ToolCallKeyword::Call.as_str()
    }

    fn is_mcp_call_callee_context(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        let expected_keyword = match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Resource) => McpCallOperation::Read.as_str(),
            Some(ReferenceKeyword::Prompt) => McpCallOperation::Render.as_str(),
            _ => return false,
        };
        let Some(reference_token) = trailing_reference_token(line_prefix) else {
            return false;
        };
        let Some(reference_start_index) = line_prefix.rfind(reference_token) else {
            return false;
        };
        let value_prefix = line_prefix[..reference_start_index].trim_end();

        if let Some((_, value_prefix_after_separator)) = value_prefix.rsplit_once(':') {
            return value_prefix_after_separator.trim().ends_with(expected_keyword);
        }

        value_prefix.ends_with(expected_keyword)
    }

    fn default_reference_completion_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> Option<Vec<CompletionSuggestion>> {
        let reference_root_keyword = reference_completion_path.root_keyword();
        let schema_reference_root = reference_completion_path.is_schema_root();
        let mcp_reference_root =
            DeclarationKeyword::from_identifier(reference_completion_path.root_identifier()) == Some(DeclarationKeyword::Mcp);

        if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
            return Some(reference_suggestions.to_vec());
        }

        if matches!(
            reference_root_keyword,
            Some(ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt)
        ) {
            return Some(reference_suggestions.to_vec());
        }

        if schema_reference_root || mcp_reference_root || reference_root_keyword.is_some() {
            return Some(reference_suggestions.to_vec());
        }

        if !reference_suggestions.is_empty() {
            return Some(reference_suggestions.to_vec());
        }

        None
    }

    fn can_suggest_reference_roots(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        let reference_token_has_trailing_separator =
            trailing_reference_token(line_prefix).is_some_and(|reference_token| reference_token.ends_with('.'));

        !reference_token_has_trailing_separator && reference_completion_path.complete_accesses.is_empty()
    }

    pub(super) fn interpolation_root_suggestions_for_context(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        root_prefix: &str,
        inside_interpolation_expression: bool,
    ) -> Vec<CompletionSuggestion> {
        let Some(position_context) = self.position_context(position) else {
            return Vec::new();
        };
        if Self::is_prompt_interpolation_reference_context(line_prefix, inside_interpolation_expression) {
            return semantic_index.prompt_interpolation_root_suggestions(root_prefix, position_context);
        }

        semantic_index.interpolation_root_suggestions(root_prefix, position_context)
    }

    fn is_prompt_interpolation_reference_context(line_prefix: &str, inside_interpolation_expression: bool) -> bool {
        if !inside_interpolation_expression {
            return false;
        }

        AgentPropertyValueCompletionContext::from_line_prefix(line_prefix)
            .is_some_and(|completion_context| completion_context.property_name == AgentExpressionPropertyName::Instruction)
    }

    pub(super) fn inference_setting_value_completion_context(
        &self,
        line_has_property_separator: bool,
        line_prefix: &str,
    ) -> Option<InferenceSettingValueCompletionContext> {
        if !line_has_property_separator {
            return None;
        }

        InferenceSettingValueCompletionContext::from_line_prefix(line_prefix)
    }

    fn reference_completion_constraint(
        &self,
        line_prefix: &str,
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> ReferenceCompletionConstraint {
        if Self::is_for_loop_iterable_clause_context(line_prefix) {
            return ReferenceCompletionConstraint::ForLoopIterable;
        }

        let line_reference_constraint = ReferenceCompletionConstraint::from_line_prefix(line_prefix);

        if line_reference_constraint == ReferenceCompletionConstraint::ForLoopIterable {
            return line_reference_constraint;
        }

        let Some(inference_value_completion_context) = inference_setting_value_completion_context else {
            return line_reference_constraint;
        };

        match inference_value_completion_context.inference_setting {
            InferenceSetting::MaxTokens
            | InferenceSetting::TopK
            | InferenceSetting::Seed
            | InferenceSetting::StuckThreshold
            | InferenceSetting::ProviderMaxRetries
            | InferenceSetting::ProviderRetryBaseDelayMs => ReferenceCompletionConstraint::InferenceIntegerValue,
            InferenceSetting::Temperature
            | InferenceSetting::TopP
            | InferenceSetting::FrequencyPenalty
            | InferenceSetting::PresencePenalty
            | InferenceSetting::RepeatPenalty => ReferenceCompletionConstraint::InferenceNumericValue,
        }
    }

    fn is_for_loop_iterable_clause_context(line_prefix: &str) -> bool {
        if ForLoopIterableValueCompletionContext::from_line_prefix(line_prefix).is_some() {
            return true;
        }

        let trimmed_line_prefix = line_prefix.trim_start();
        let agent_keyword_with_space = format!("{} ", DeclarationKeyword::Agent.as_str());
        let Some(after_agent_keyword) = trimmed_line_prefix.strip_prefix(agent_keyword_with_space.as_str()) else {
            return false;
        };

        let for_keyword_with_spaces = format!(" {} ", ForClauseKeyword::For.as_str());
        let in_keyword_with_spaces = format!(" {} ", ForClauseKeyword::In.as_str());

        after_agent_keyword.contains(for_keyword_with_spaces.as_str()) && after_agent_keyword.contains(in_keyword_with_spaces.as_str())
    }
}
