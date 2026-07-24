use lsp_types::{Position, Range};

use super::DocumentState;

impl DocumentState {
    #[must_use]
    pub fn definition_range(&self, position: Position) -> Option<Range> {
        let symbol_token_at_position = self.symbol_token_at_position(position)?;
        let definition_span = self.semantic_snapshot.semantic_index.definition_span_for_symbol_at_cursor(
            symbol_token_at_position.symbol_token.as_str(),
            symbol_token_at_position.cursor_character_offset,
            self.position_context(position)?,
        )?;

        Some(self.range_for_source_span(definition_span))
    }
}
