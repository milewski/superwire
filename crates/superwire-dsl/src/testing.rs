use std::fmt::Write as _;

use crate::{parse_workflow, DslParseError, Workflow};

pub const COMPACT_CURSOR_MARKER: &str = "<cursor>";
pub const SPACED_CURSOR_MARKER: &str = "< cursor >";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineCursorPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowSourceTemplate {
    source_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowSourceWithCursor {
    source_text: String,
    cursor_position: InlineCursorPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAssertion {
    pub name: String,
    pub expected: String,
    pub actual: String,
}

impl WorkflowSourceTemplate {
    #[must_use]
    pub fn from_inline(source_text: impl Into<String>) -> Self {
        Self {
            source_text: normalize_rust_doc_comment_tokens(&source_text.into()),
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source_text
    }

    pub fn parse_workflow(&self) -> Result<Workflow, DslParseError> {
        parse_workflow(&self.source_text)
    }

    #[must_use]
    pub fn normalized_cursor_layout(&self) -> Self {
        Self {
            source_text: normalize_inline_cursor_layout(&self.source_text),
        }
    }

    #[must_use]
    pub fn without_cursor_normalization(&self) -> WorkflowSourceWithCursor {
        source_without_cursor_normalization(&self.source_text)
    }

    #[must_use]
    pub fn with_cursor(&self) -> WorkflowSourceWithCursor {
        self.normalized_cursor_layout().without_cursor_normalization()
    }
}

impl WorkflowSourceWithCursor {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source_text
    }

    #[must_use]
    pub fn cursor_position(&self) -> InlineCursorPosition {
        self.cursor_position
    }
}

impl SnapshotAssertion {
    #[must_use]
    pub fn new(name: impl Into<String>, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn assert_matches(&self) {
        assert_eq!(
            self.actual,
            self.expected,
            "snapshot `{}` did not match\n{}",
            self.name,
            stable_text_diff(&self.expected, &self.actual)
        );
    }
}

#[must_use]
pub fn normalize_rust_doc_comment_tokens(source_template: &str) -> String {
    let mut normalized_source = String::new();
    let mut remaining_source = source_template;

    while let Some(doc_attribute_start) = remaining_source.find("#[doc = r\"") {
        normalized_source.push_str(&remaining_source[..doc_attribute_start]);
        remaining_source = &remaining_source[doc_attribute_start + "#[doc = r\"".len()..];

        let Some(doc_attribute_end) = remaining_source.find("\"]") else {
            normalized_source.push_str("#[doc = r\"");
            normalized_source.push_str(remaining_source);

            return normalized_source;
        };

        normalized_source.push_str("///");
        normalized_source.push_str(&remaining_source[..doc_attribute_end]);
        normalized_source.push('\n');
        remaining_source = &remaining_source[doc_attribute_end + "\"]".len()..];
    }

    normalized_source.push_str(remaining_source);
    normalized_source
}

#[must_use]
pub fn normalize_inline_cursor_layout(source_template: &str) -> String {
    let compact_marker_offset = source_template.find(COMPACT_CURSOR_MARKER);
    let spaced_marker_offset = source_template.find(SPACED_CURSOR_MARKER);

    let (marker, marker_offset) = match (compact_marker_offset, spaced_marker_offset) {
        (Some(compact_offset), Some(spaced_offset)) => {
            if compact_offset <= spaced_offset {
                (COMPACT_CURSOR_MARKER, compact_offset)
            } else {
                (SPACED_CURSOR_MARKER, spaced_offset)
            }
        }
        (Some(compact_offset), None) => (COMPACT_CURSOR_MARKER, compact_offset),
        (None, Some(spaced_offset)) => (SPACED_CURSOR_MARKER, spaced_offset),
        (None, None) => {
            return source_template.to_string();
        }
    };

    if is_inside_string_literal(source_template, marker_offset) {
        return source_template.to_string();
    }

    let previous_character = source_template[..marker_offset]
        .chars()
        .rev()
        .find(|character| !character.is_whitespace());

    if previous_character == Some('.') || previous_character == Some(':') || previous_character == Some('(') {
        return source_template.to_string();
    }

    let mut normalized_source = String::new();
    normalized_source.push_str(&source_template[..marker_offset]);

    if !normalized_source.ends_with('\n') {
        normalized_source.push('\n');
    }

    normalized_source.push_str(marker);

    let marker_end_offset = marker_offset + marker.len();
    let remaining_source = &source_template[marker_end_offset..];
    let next_character = remaining_source.chars().find(|character| !character.is_whitespace());

    if next_character == Some('{') {
        return source_template.to_string();
    }

    if next_character == Some('}') {
        normalized_source.push('\n');
    }

    normalized_source.push_str(remaining_source);

    merge_lone_opening_brace_lines(&normalized_source)
}

fn source_without_cursor_normalization(source_template: &str) -> WorkflowSourceWithCursor {
    let (cursor_marker, cursor_byte_offset) = if let Some(marker_offset) = source_template.find(COMPACT_CURSOR_MARKER) {
        (COMPACT_CURSOR_MARKER, marker_offset)
    } else {
        panic!("cursor marker should exist in test source");
    };

    let mut line = 0_u32;
    let mut character = 0_u32;

    for character_in_source in source_template[..cursor_byte_offset].chars() {
        if character_in_source == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    let source_text = source_template.replacen(cursor_marker, "", 1);

    WorkflowSourceWithCursor {
        source_text,
        cursor_position: InlineCursorPosition { line, character },
    }
}

fn merge_lone_opening_brace_lines(source_text: &str) -> String {
    let mut source_lines = source_text.lines().map(str::to_string).collect::<Vec<_>>();
    let mut line_index = 0_usize;

    while line_index < source_lines.len() {
        if line_index == 0 {
            line_index += 1;

            continue;
        }

        if source_lines[line_index].trim() != "{" {
            line_index += 1;

            continue;
        }

        if !source_lines[line_index - 1].is_empty() {
            source_lines[line_index - 1].push(' ');
        }

        source_lines[line_index - 1].push('{');
        let _ = source_lines.remove(line_index);
    }

    source_lines.join("\n")
}

fn is_inside_string_literal(source_text: &str, byte_offset: usize) -> bool {
    let mut inside_string = false;
    let mut escaping = false;

    for character in source_text[..byte_offset].chars() {
        if escaping {
            escaping = false;

            continue;
        }

        if inside_string {
            if character == '\\' {
                escaping = true;

                continue;
            }

            if character == '"' {
                inside_string = false;
            }

            continue;
        }

        if character == '"' {
            inside_string = true;
        }
    }

    inside_string
}

fn stable_text_diff(expected: &str, actual: &str) -> String {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let max_line_count = expected_lines.len().max(actual_lines.len());
    let mut difference_text = String::new();

    for line_index in 0..max_line_count {
        let expected_line = expected_lines.get(line_index).copied();
        let actual_line = actual_lines.get(line_index).copied();

        if expected_line == actual_line {
            continue;
        }

        let _ = writeln!(difference_text, "line {}:", line_index + 1);

        match expected_line {
            Some(line) => {
                let _ = writeln!(difference_text, "  expected: {line}");
            }
            None => difference_text.push_str("  expected: <missing>\n"),
        }

        match actual_line {
            Some(line) => {
                let _ = writeln!(difference_text, "  actual:   {line}");
            }
            None => difference_text.push_str("  actual:   <missing>\n"),
        }
    }

    difference_text
}
