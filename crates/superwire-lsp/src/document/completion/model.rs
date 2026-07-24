use lsp_types::Position;

use super::super::scope::{model_property_scope_suggestions, CompletionScope};
use super::super::semantic_index::SemanticIndex;
use super::super::{CompletionSuggestion, DocumentState};

impl DocumentState {
    pub(super) fn model_property_suggestions_at_position(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        completion_scope: CompletionScope,
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if inside_interpolation_expression
            || !matches!(completion_scope, CompletionScope::ModelProperties | CompletionScope::General)
            || semantic_index.model_name_at_position(self.position_context(position)?).is_none()
            || (line_has_property_separator && !Self::line_prefix_ends_after_property_value(line_prefix))
            || Self::line_prefix_has_open_property_string_value(line_prefix)
        {
            return None;
        }

        Some(model_property_scope_suggestions(line_prefix))
    }
}
