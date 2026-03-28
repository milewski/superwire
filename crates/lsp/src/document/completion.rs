use engine_ai_core::dsl::{parse_workflow, AgentExpressionPropertyName, ReferenceKeyword};

use crate::protocol::Position;

use super::completion_context::{
    AgentPropertyValueCompletionContext, DeclarationHeaderCompletionContext, InferenceSettingValueCompletionContext,
    ModelCallCompletionContext,
};
use super::position::byte_offset_for_position;
use super::reference::{ReferenceCompletionConstraint, ReferenceCompletionPath};
use super::scope::{agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions, CompletionScope};
use super::semantic_index::SemanticIndex;
use super::text_utils::{is_inside_interpolation_expression, is_inside_multiline_string_literal, trailing_reference_token};
use super::{CompletionSuggestion, DocumentState};
use engine_ai_core::runtime::InferenceSetting;

const COMPLETION_RECOVERY_PLACEHOLDER: &str = "__completion_placeholder";

impl DocumentState {
    #[must_use]
    pub fn completion_suggestions(&self, position: Position) -> Vec<CompletionSuggestion> {
        let Some(line_prefix) = self.line_prefix(position) else {
            return Vec::new();
        };

        let inside_interpolation_expression = is_inside_interpolation_expression(&line_prefix);

        if self.is_inside_multiline_string_literal(position) && !inside_interpolation_expression {
            return Vec::new();
        }

        let completion_scope = self.completion_scope(position);
        let semantic_index = self.semantic_index_for_completion(position);

        if let Some(typed_declaration_suggestions) =
            self.typed_declaration_scope_suggestions(completion_scope, &line_prefix, position, &semantic_index)
        {
            return typed_declaration_suggestions;
        }

        let line_has_property_separator = line_prefix.trim_start().contains(':');
        let should_include_builtin_function_suggestions = line_has_property_separator || inside_interpolation_expression;
        let inference_setting_value_completion_context =
            self.inference_setting_value_completion_context(line_has_property_separator, &line_prefix);

        if let Some(non_reference_suggestions) = self.non_reference_suggestions(
            &semantic_index,
            &line_prefix,
            position,
            completion_scope,
            line_has_property_separator,
            inside_interpolation_expression,
        ) {
            return non_reference_suggestions;
        }

        if let Some(reference_suggestions) = self.reference_completion_suggestions(
            &semantic_index,
            &line_prefix,
            position,
            inside_interpolation_expression,
            inference_setting_value_completion_context.as_ref(),
        ) {
            return reference_suggestions;
        }

        if inside_interpolation_expression {
            return semantic_index.interpolation_root_suggestions("");
        }

        if semantic_index.is_type_position(position, &line_prefix) {
            let current_schema_name = semantic_index.schema_name_at_position(position);
            let type_suggestions = semantic_index.type_suggestions(&line_prefix, current_schema_name);

            if !type_suggestions.is_empty() {
                return type_suggestions;
            }
        }

        semantic_index.default_suggestions(should_include_builtin_function_suggestions)
    }

    fn typed_declaration_scope_suggestions(
        &self,
        completion_scope: CompletionScope,
        line_prefix: &str,
        position: Position,
        semantic_index: &SemanticIndex,
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::TypedDeclarations {
            return None;
        }

        if !line_prefix.contains(':') {
            return Some(Vec::new());
        }

        let current_schema_name = semantic_index.schema_name_at_position(position);

        Some(semantic_index.type_suggestions(line_prefix, current_schema_name))
    }

    fn non_reference_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        completion_scope: CompletionScope,
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope == CompletionScope::General
            && !line_has_property_separator
            && !inside_interpolation_expression
            && semantic_index.is_output_position(position)
        {
            return Some(Vec::new());
        }

        if line_has_property_separator {
            if let Some(inference_value_completion_context) = InferenceSettingValueCompletionContext::from_line_prefix(line_prefix) {
                if inference_value_completion_context.inside_string_literal {
                    return Some(Vec::new());
                }

                if ReferenceCompletionPath::from_line_prefix(line_prefix).is_none() {
                    if inference_value_completion_context.value_prefix.is_empty() {
                        return Some(semantic_index.inference_value_root_suggestions(""));
                    }

                    return Some(Vec::new());
                }
            }
        }

        if semantic_index.agent_name_at_position(position).is_some() && line_has_property_separator && !inside_interpolation_expression {
            if let Some(context_property_suggestions) = self.context_property_value_suggestions(semantic_index, line_prefix) {
                return Some(context_property_suggestions);
            }
        }

        if !line_has_property_separator && !inside_interpolation_expression {
            if DeclarationHeaderCompletionContext::from_line_prefix(line_prefix).is_some() {
                return Some(Vec::new());
            }

            match completion_scope {
                CompletionScope::InferenceSettings => {
                    return Some(inference_setting_scope_suggestions(line_prefix));
                }
                CompletionScope::AgentProperties => {
                    return Some(agent_property_scope_suggestions(line_prefix));
                }
                CompletionScope::General | CompletionScope::TypedDeclarations => {}
            }

            if completion_scope == CompletionScope::General && semantic_index.is_root_declaration_position(position) {
                return Some(semantic_index.root_declaration_suggestions(line_prefix));
            }
        }

        if let Some(model_call_context) = ModelCallCompletionContext::from_line_prefix(line_prefix) {
            let mut model_suggestions = semantic_index.model_call_suggestions(&model_call_context);

            if !model_call_context.inside_string_literal {
                model_suggestions.extend(semantic_index.model_value_root_suggestions(&model_call_context.model_prefix));
                model_suggestions.sort_by(|left_suggestion, right_suggestion| {
                    left_suggestion
                        .label
                        .cmp(&right_suggestion.label)
                        .then_with(|| left_suggestion.insert_text.cmp(&right_suggestion.insert_text))
                });
                model_suggestions.dedup_by(|left_suggestion, right_suggestion| {
                    left_suggestion.label == right_suggestion.label
                        && left_suggestion.insert_text == right_suggestion.insert_text
                        && left_suggestion.detail == right_suggestion.detail
                });
            }

            if !model_suggestions.is_empty() {
                return Some(model_suggestions);
            }
        }

        if let Some(provider_driver_suggestions) = semantic_index.provider_driver_value_suggestions(position, line_prefix) {
            return Some(provider_driver_suggestions);
        }

        if let Some(provider_property_suggestions) = semantic_index.provider_property_suggestions(position, line_prefix) {
            return Some(provider_property_suggestions);
        }

        None
    }

    fn context_property_value_suggestions(&self, semantic_index: &SemanticIndex, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let agent_property_value_completion_context = AgentPropertyValueCompletionContext::from_line_prefix(line_prefix)?;

        if agent_property_value_completion_context.property_name != AgentExpressionPropertyName::Context {
            return None;
        }

        Some(semantic_index.context_function_suggestions(&agent_property_value_completion_context.value_prefix))
    }

    fn reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        inside_interpolation_expression: bool,
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> Option<Vec<CompletionSuggestion>> {
        let reference_completion_path = ReferenceCompletionPath::from_line_prefix(line_prefix)?;
        let reference_completion_constraint = self.reference_completion_constraint(line_prefix, inference_setting_value_completion_context);
        let reference_suggestions =
            semantic_index.reference_path_suggestions(&reference_completion_path, reference_completion_constraint, position);
        let reference_root_keyword = reference_completion_path.root_keyword();
        let schema_reference_root = reference_completion_path.is_schema_root();

        if inference_setting_value_completion_context.is_some() {
            let reference_token_has_trailing_separator =
                trailing_reference_token(line_prefix).is_some_and(|reference_token| reference_token.ends_with('.'));
            let can_suggest_inference_roots =
                !reference_token_has_trailing_separator && reference_completion_path.complete_accesses.is_empty();

            match reference_completion_path.root_keyword() {
                Some(ReferenceKeyword::Input | ReferenceKeyword::Agent) => {
                    if can_suggest_inference_roots {
                        return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                    }

                    return Some(reference_suggestions);
                }
                Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool) | None => {
                    if can_suggest_inference_roots {
                        return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                    }

                    return Some(Vec::new());
                }
            }
        }

        if inside_interpolation_expression {
            let reference_token_has_trailing_separator =
                trailing_reference_token(line_prefix).is_some_and(|reference_token| reference_token.ends_with('.'));
            let can_suggest_interpolation_roots =
                !reference_token_has_trailing_separator && reference_completion_path.complete_accesses.is_empty();

            match reference_completion_path.root_keyword() {
                Some(ReferenceKeyword::Input | ReferenceKeyword::Agent) => {
                    if can_suggest_interpolation_roots {
                        return Some(semantic_index.interpolation_root_suggestions(reference_completion_path.root_identifier()));
                    }

                    return Some(reference_suggestions);
                }
                Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool) | None => {
                    if can_suggest_interpolation_roots {
                        return Some(semantic_index.interpolation_root_suggestions(reference_completion_path.root_identifier()));
                    }

                    return Some(Vec::new());
                }
            }
        }

        if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
            return Some(reference_suggestions);
        }

        if reference_root_keyword == Some(ReferenceKeyword::Tool) {
            return Some(reference_suggestions);
        }

        if schema_reference_root || reference_root_keyword.is_some() {
            return Some(reference_suggestions);
        }

        if !reference_suggestions.is_empty() {
            return Some(reference_suggestions);
        }

        None
    }

    fn inference_setting_value_completion_context(
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

    fn semantic_index_for_completion(&self, position: Position) -> SemanticIndex {
        if self.semantic_snapshot.parse_error.is_none() {
            return self.semantic_snapshot.semantic_index.clone();
        }

        self.recovered_semantic_index(position)
            .unwrap_or_else(|| self.semantic_snapshot.semantic_index.clone())
    }

    fn recovered_semantic_index(&self, position: Position) -> Option<SemanticIndex> {
        let cursor_offset = byte_offset_for_position(&self.text, position)?;

        let mut recovered_source = String::with_capacity(self.text.len() + COMPLETION_RECOVERY_PLACEHOLDER.len());
        recovered_source.push_str(&self.text[..cursor_offset]);
        recovered_source.push_str(COMPLETION_RECOVERY_PLACEHOLDER);
        recovered_source.push_str(&self.text[cursor_offset..]);

        let workflow = parse_workflow(&recovered_source).ok()?;

        Some(SemanticIndex::from_workflow(&workflow))
    }

    fn completion_scope(&self, position: Position) -> CompletionScope {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return CompletionScope::General;
        };

        completion_scope_at_offset(&self.text, cursor_offset)
    }

    fn is_inside_multiline_string_literal(&self, position: Position) -> bool {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return false;
        };

        is_inside_multiline_string_literal(&self.text, cursor_offset)
    }
}
