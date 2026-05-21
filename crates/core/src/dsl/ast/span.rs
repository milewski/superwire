use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

impl SourcePosition {
    #[must_use]
    pub fn to_byte_offset(self, source_text: &str) -> Option<usize> {
        if self.line == 0 || self.column == 0 {
            return None;
        }

        let mut current_line_number = 1_usize;
        let mut current_column_number = 1_usize;

        for (byte_offset, character) in source_text.char_indices() {
            if current_line_number == self.line && current_column_number == self.column {
                return Some(byte_offset);
            }

            if character == '\n' {
                current_line_number += 1;
                current_column_number = 1;

                continue;
            }

            current_column_number += 1;
        }

        if current_line_number == self.line && current_column_number == self.column {
            return Some(source_text.len());
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceSpan {
    #[must_use]
    pub fn to_byte_range(self, source_text: &str) -> Option<Range<usize>> {
        let start_byte_offset = self.start.to_byte_offset(source_text)?;
        let mut end_byte_offset = self.end.to_byte_offset(source_text)?;

        if end_byte_offset < start_byte_offset {
            return None;
        }

        if end_byte_offset == start_byte_offset {
            if let Some(character_at_start) = source_text[start_byte_offset..].chars().next() {
                end_byte_offset = start_byte_offset + character_at_start.len_utf8();
            }
        }

        Some(start_byte_offset..end_byte_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::{SourcePosition, SourceSpan};

    #[test]
    fn maps_source_position_to_byte_offset() {
        let source_text = "alpha\nbeta\n";

        assert_eq!(SourcePosition { line: 1, column: 1 }.to_byte_offset(source_text), Some(0));
        assert_eq!(SourcePosition { line: 2, column: 1 }.to_byte_offset(source_text), Some(6));
        assert_eq!(SourcePosition { line: 2, column: 5 }.to_byte_offset(source_text), Some(10));
        assert_eq!(SourcePosition { line: 3, column: 1 }.to_byte_offset(source_text), Some(11));
    }

    #[test]
    fn maps_source_span_to_byte_range() {
        let source_text = "agent greeting";
        let source_span = SourceSpan {
            start: SourcePosition { line: 1, column: 7 },
            end: SourcePosition { line: 1, column: 15 },
        };

        assert_eq!(source_span.to_byte_range(source_text), Some(6..14));
    }
}
