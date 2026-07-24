use lsp_types::{Position, PositionEncodingKind, Range};
use superwire_dsl::{SourcePosition, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

impl PositionEncoding {
    #[must_use]
    pub fn from_kind(position_encoding_kind: &PositionEncodingKind) -> Option<Self> {
        if position_encoding_kind == &PositionEncodingKind::UTF8 {
            return Some(Self::Utf8);
        }

        if position_encoding_kind == &PositionEncodingKind::UTF16 {
            return Some(Self::Utf16);
        }

        if position_encoding_kind == &PositionEncodingKind::UTF32 {
            return Some(Self::Utf32);
        }

        None
    }

    #[must_use]
    pub fn as_kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
            Self::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    fn character_width(self, character: char) -> usize {
        match self {
            Self::Utf8 => character.len_utf8(),
            Self::Utf16 => character.len_utf16(),
            Self::Utf32 => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineIndex {
    line_start_offsets: Vec<usize>,
    position_encoding: PositionEncoding,
}

impl LineIndex {
    #[must_use]
    pub fn new(source_text: &str, position_encoding: PositionEncoding) -> Self {
        let mut line_start_offsets = Vec::with_capacity(source_text.lines().count().saturating_add(1));
        line_start_offsets.push(0);

        for (byte_offset, character) in source_text.char_indices() {
            if character == '\n' {
                line_start_offsets.push(byte_offset.saturating_add(1));
            }
        }

        Self {
            line_start_offsets,
            position_encoding,
        }
    }

    #[must_use]
    pub fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    #[must_use]
    pub fn byte_offset(&self, source_text: &str, position: Position) -> Option<usize> {
        let line_index = usize::try_from(position.line).ok()?;
        let line_range = self.line_content_byte_range(source_text, line_index)?;
        let line_text = source_text.get(line_range.clone())?;
        let target_character_offset = usize::try_from(position.character).ok()?;
        let mut current_character_offset = 0_usize;

        for (relative_byte_offset, character) in line_text.char_indices() {
            if current_character_offset == target_character_offset {
                return Some(line_range.start.saturating_add(relative_byte_offset));
            }

            current_character_offset = current_character_offset.saturating_add(self.position_encoding.character_width(character));

            if current_character_offset > target_character_offset {
                return None;
            }
        }

        (current_character_offset == target_character_offset).then_some(line_range.end)
    }

    #[must_use]
    pub fn position(&self, source_text: &str, byte_offset: usize) -> Option<Position> {
        if byte_offset > source_text.len() || !source_text.is_char_boundary(byte_offset) {
            return None;
        }

        let line_index = self
            .line_start_offsets
            .partition_point(|line_start_offset| *line_start_offset <= byte_offset)
            .saturating_sub(1);
        let line_range = self.line_content_byte_range(source_text, line_index)?;
        let bounded_byte_offset = byte_offset.min(line_range.end);
        let line_prefix = source_text.get(line_range.start..bounded_byte_offset)?;
        let character_offset = line_prefix.chars().fold(0_usize, |offset, character| {
            offset.saturating_add(self.position_encoding.character_width(character))
        });

        Some(Position {
            line: u32_from_usize_saturating(line_index),
            character: u32_from_usize_saturating(character_offset),
        })
    }

    #[must_use]
    pub fn range(&self, source_text: &str, start_byte_offset: usize, end_byte_offset: usize) -> Option<Range> {
        let start = self.position(source_text, start_byte_offset)?;
        let end = self.position(source_text, end_byte_offset)?;

        Some(Range { start, end })
    }

    #[must_use]
    pub fn source_span_range(&self, source_text: &str, source_span: SourceSpan) -> Range {
        let Some(mut byte_range) = source_span.to_byte_range(source_text) else {
            return self.zero_range();
        };

        if byte_range.end <= byte_range.start {
            byte_range.end = source_text
                .get(byte_range.start..)
                .and_then(|source_suffix| source_suffix.chars().next())
                .map_or(byte_range.start, |character| byte_range.start.saturating_add(character.len_utf8()));
        }

        self.range(source_text, byte_range.start, byte_range.end)
            .unwrap_or_else(|| self.zero_range())
    }

    #[must_use]
    pub fn line_prefix<'source>(&self, source_text: &'source str, position: Position) -> Option<&'source str> {
        let byte_offset = self.byte_offset(source_text, position)?;
        let line_index = usize::try_from(position.line).ok()?;
        let line_range = self.line_content_byte_range(source_text, line_index)?;

        source_text.get(line_range.start..byte_offset)
    }

    #[must_use]
    pub fn line_suffix<'source>(&self, source_text: &'source str, position: Position) -> Option<&'source str> {
        let byte_offset = self.byte_offset(source_text, position)?;
        let line_index = usize::try_from(position.line).ok()?;
        let line_range = self.line_content_byte_range(source_text, line_index)?;

        source_text.get(byte_offset..line_range.end)
    }

    #[must_use]
    pub fn line_content_byte_range(&self, source_text: &str, line_index: usize) -> Option<std::ops::Range<usize>> {
        let line_start_offset = *self.line_start_offsets.get(line_index)?;
        let next_line_start_offset = self
            .line_start_offsets
            .get(line_index.saturating_add(1))
            .copied()
            .unwrap_or(source_text.len());
        let mut line_end_offset = next_line_start_offset;

        if line_end_offset > line_start_offset && source_text.as_bytes().get(line_end_offset.saturating_sub(1)) == Some(&b'\n') {
            line_end_offset = line_end_offset.saturating_sub(1);
        }

        if line_end_offset > line_start_offset && source_text.as_bytes().get(line_end_offset.saturating_sub(1)) == Some(&b'\r') {
            line_end_offset = line_end_offset.saturating_sub(1);
        }

        Some(line_start_offset..line_end_offset)
    }

    #[must_use]
    pub fn next_line_start_byte_offset(&self, line_index: usize) -> Option<usize> {
        self.line_start_offsets.get(line_index.saturating_add(1)).copied()
    }

    #[must_use]
    pub fn zero_range(&self) -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DocumentPosition<'document> {
    source_text: &'document str,
    byte_offset: usize,
}

impl<'document> DocumentPosition<'document> {
    #[must_use]
    pub fn new(source_text: &'document str, line_index: &'document LineIndex, position: Position) -> Option<Self> {
        let byte_offset = line_index.byte_offset(source_text, position)?;

        Some(Self { source_text, byte_offset })
    }

    #[must_use]
    pub fn byte_offset(self) -> usize {
        self.byte_offset
    }

    #[must_use]
    pub fn contains(self, source_span: SourceSpan) -> bool {
        let source_prefix = self.source_text.get(..self.byte_offset).unwrap_or_default();
        let line_start_byte_offset = source_prefix
            .rfind('\n')
            .map_or(0, |line_ending_byte_offset| line_ending_byte_offset.saturating_add(1));
        let cursor_source_position = SourcePosition {
            line: source_prefix.bytes().filter(|byte| *byte == b'\n').count().saturating_add(1),
            column: self
                .source_text
                .get(line_start_byte_offset..self.byte_offset)
                .map_or(1, |line_prefix| line_prefix.chars().count().saturating_add(1)),
        };

        source_span.contains_position(cursor_source_position)
    }
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
