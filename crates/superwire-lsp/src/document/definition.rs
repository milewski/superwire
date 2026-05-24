use lsp_types::{Position, Range};

use super::position::source_span_to_range;
use super::DocumentState;

impl DocumentState {
    #[must_use]
    pub fn definition_range(&self, position: Position) -> Option<Range> {
        let symbol_token_at_position = self.symbol_token_at_position(position)?;
        let definition_span = self.semantic_snapshot.semantic_index.definition_span_for_symbol_at_cursor(
            symbol_token_at_position.symbol_token.as_str(),
            symbol_token_at_position.cursor_character_offset,
            position,
        )?;

        Some(source_span_to_range(&self.text, definition_span))
    }
}
