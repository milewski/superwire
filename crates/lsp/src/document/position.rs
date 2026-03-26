use engine_ai_core::dsl::{SourcePosition, SourceSpan};

use crate::protocol::{Position, Range};

pub fn byte_offset_for_position(source_text: &str, position: Position) -> Option<usize> {
    let target_line = position.line as usize;
    let target_character = position.character as usize;

    let mut current_line = 0_usize;
    let mut current_character = 0_usize;

    for (byte_offset, character) in source_text.char_indices() {
        if current_line == target_line && current_character == target_character {
            return Some(byte_offset);
        }

        if character == '\n' {
            if current_line == target_line {
                return Some(byte_offset);
            }

            current_line += 1;
            current_character = 0;
            continue;
        }

        if current_line == target_line {
            current_character += 1;
        }
    }

    if current_line == target_line {
        return Some(source_text.len());
    }

    None
}

pub fn source_span_to_range(source_text: &str, source_span: SourceSpan) -> Range {
    let start = source_position_to_position(source_span.start);
    let mut end = source_position_to_position(source_span.end);

    if end.line < start.line || (end.line == start.line && end.character <= start.character) {
        end = Position {
            line: start.line,
            character: start.character.saturating_add(1),
        };

        if let Some(line_length) = line_character_count(source_text, start.line) {
            end.character = end.character.min(u32_from_usize_saturating(line_length));
        }
    }

    Range { start, end }
}

pub fn source_span_contains_position(source_span: SourceSpan, position: Position) -> bool {
    let target_line = position.line as usize + 1;
    let target_column = position.character as usize + 1;

    let starts_before_or_at =
        (source_span.start.line < target_line) || (source_span.start.line == target_line && source_span.start.column <= target_column);

    let ends_after_or_at =
        (source_span.end.line > target_line) || (source_span.end.line == target_line && source_span.end.column >= target_column);

    starts_before_or_at && ends_after_or_at
}

pub fn zero_range() -> Range {
    Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 1 },
    }
}

fn line_character_count(source_text: &str, line_index: u32) -> Option<usize> {
    source_text
        .lines()
        .nth(line_index as usize)
        .map(|line_text| line_text.chars().count())
}

fn source_position_to_position(source_position: SourcePosition) -> Position {
    Position {
        line: u32_from_usize_saturating(source_position.line.saturating_sub(1)),
        character: u32_from_usize_saturating(source_position.column.saturating_sub(1)),
    }
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
