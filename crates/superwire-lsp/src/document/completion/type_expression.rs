use lsp_types::Position;
use superwire_dsl::ToolPropertyName;

use super::super::completion_context::{ArrayFixedLengthCompletionContext, ValueCompletionContext};
use super::super::scope::CompletionScope;
use super::super::semantic_index::SemanticIndex;
use super::super::{CompletionSuggestion, DocumentState};

impl DocumentState {
    pub(super) fn is_typed_description_string_literal_context(
        line_prefix: &str,
        completion_scope: CompletionScope,
        _semantic_index: &SemanticIndex,
        _position: Position,
    ) -> bool {
        let trimmed_line_prefix = line_prefix.trim_start();
        let Some((_line_before_value, value_prefix)) = trimmed_line_prefix.rsplit_once(':') else {
            return false;
        };

        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        if !value_completion_context.inside_string_literal {
            return false;
        }

        if completion_scope != CompletionScope::TypedDeclarations {
            return false;
        }

        let trimmed_value_prefix = value_prefix.trim_start();
        let Some(last_quote_index) = trimmed_value_prefix.rfind('"') else {
            return false;
        };

        let value_before_open_quote = trimmed_value_prefix[..last_quote_index].trim_end();

        !value_before_open_quote.is_empty()
    }

    pub(super) fn typed_declaration_scope_suggestions(
        &self,
        completion_scope: CompletionScope,
        line_prefix: &str,
        position: Position,
        semantic_index: &SemanticIndex,
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::TypedDeclarations {
            return None;
        }

        if self.tool_schema_property_name_at_position(position) == Some(ToolPropertyName::Bindings) {
            return None;
        }

        if !line_prefix.contains(':') {
            let trimmed_prefix = line_prefix.trim_start();

            if let Some(mcp_tool_schema_source) = self.mcp_tool_schema_source_at_position(position, semantic_index) {
                let mcp_suggestions =
                    self.mcp_tool_schema_field_suggestions_for_source(semantic_index, position, trimmed_prefix, mcp_tool_schema_source);

                if !mcp_suggestions.is_empty() {
                    return Some(mcp_suggestions);
                }
            }

            return Some(Vec::new());
        }

        if ArrayFixedLengthCompletionContext::from_line_prefix(line_prefix).is_some() {
            return Some(Vec::new());
        }

        let current_schema_name = semantic_index.schema_name_at_position(self.position_context(position)?);

        Some(semantic_index.type_suggestions(line_prefix, current_schema_name))
    }
}
