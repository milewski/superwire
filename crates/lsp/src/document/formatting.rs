use engine_ai_core::dsl::format_workflow_source;

use crate::protocol::{Position, Range};

use super::{DocumentFormattingEdit, DocumentState};

impl DocumentState {
    #[must_use]
    pub fn formatting_edit(&self) -> Option<DocumentFormattingEdit> {
        let formatted_text = format_workflow_source(&self.text).ok()?;

        if formatted_text == self.text {
            return None;
        }

        Some(DocumentFormattingEdit {
            range: self.full_document_range(),
            new_text: formatted_text,
        })
    }

    fn full_document_range(&self) -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: self.document_end_position(),
        }
    }

    fn document_end_position(&self) -> Position {
        let source_lines = self.text.split('\n').collect::<Vec<_>>();

        let end_line = source_lines.len().saturating_sub(1);
        let end_character = source_lines.last().map_or(0, |line_text| line_text.chars().count());

        Position {
            line: u32_from_usize_saturating(end_line),
            character: u32_from_usize_saturating(end_character),
        }
    }
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
