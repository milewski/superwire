use engine_ai_core::dsl::{parse_workflow, ReferenceKeyword};

use crate::protocol::Position;

use super::completion_context::ModelCallCompletionContext;
use super::position::byte_offset_for_position;
use super::reference::{ReferenceCompletionConstraint, ReferenceCompletionPath};
use super::scope::{agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions, CompletionScope};
use super::semantic_index::SemanticIndex;
use super::text_utils::{is_inside_interpolation_expression, is_inside_multiline_string_literal};
use super::{CompletionSuggestion, DocumentState};

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

        if completion_scope == CompletionScope::TypedDeclarations {
            if !line_prefix.contains(':') {
                return Vec::new();
            }

            let semantic_index = self.semantic_index_for_completion(position);
            let current_schema_name = semantic_index.schema_name_at_position(position);

            return semantic_index.type_suggestions(&line_prefix, current_schema_name);
        }

        let semantic_index = self.semantic_index_for_completion(position);
        let line_has_property_separator = line_prefix.trim_start().contains(':');
        let should_include_builtin_function_suggestions = line_has_property_separator || inside_interpolation_expression;

        if !line_has_property_separator && !inside_interpolation_expression {
            match completion_scope {
                CompletionScope::InferenceSettings => {
                    return inference_setting_scope_suggestions(&line_prefix);
                }
                CompletionScope::AgentProperties => {
                    return agent_property_scope_suggestions(&line_prefix);
                }
                CompletionScope::General | CompletionScope::TypedDeclarations => {}
            }
        }

        if let Some(model_call_context) = ModelCallCompletionContext::from_line_prefix(&line_prefix) {
            let model_suggestions = semantic_index.model_call_suggestions(&model_call_context);

            if !model_suggestions.is_empty() {
                return model_suggestions;
            }
        }

        if let Some(provider_driver_suggestions) = semantic_index.provider_driver_value_suggestions(position, &line_prefix) {
            return provider_driver_suggestions;
        }

        if let Some(provider_property_suggestions) = semantic_index.provider_property_suggestions(position, &line_prefix) {
            return provider_property_suggestions;
        }

        if let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(&line_prefix) {
            let reference_completion_constraint = ReferenceCompletionConstraint::from_line_prefix(&line_prefix);
            let reference_suggestions =
                semantic_index.reference_path_suggestions(&reference_completion_path, reference_completion_constraint, position);

            if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
                return reference_suggestions;
            }

            if reference_completion_path.root_keyword() == Some(ReferenceKeyword::Tool) {
                return reference_suggestions;
            }

            if !reference_suggestions.is_empty() {
                return reference_suggestions;
            }
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
