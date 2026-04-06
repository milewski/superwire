use superwire_core::dsl::parse_workflow;

use crate::protocol::{Position, Range};

use super::{DocumentFormattingEdit, DocumentState};

impl DocumentState {
    #[must_use]
    pub fn formatting_edit(&self) -> Option<DocumentFormattingEdit> {
        let _workflow = parse_workflow(&self.text).ok()?;
        let formatted_text = self.simple_formatted_source();

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

    fn simple_formatted_source(&self) -> String {
        let mut formatted_lines = Vec::<String>::new();
        let mut indentation_depth = 0_i32;

        for source_line in self.text.lines() {
            let trimmed_line = source_line.trim();

            if trimmed_line.is_empty() {
                formatted_lines.push(String::new());

                continue;
            }

            if trimmed_line.starts_with('}') {
                indentation_depth = (indentation_depth - 1).max(0);
            }

            let mut formatted_line = "    ".repeat(usize::try_from(indentation_depth).unwrap_or(0));
            formatted_line.push_str(trimmed_line);
            formatted_lines.push(formatted_line);

            let opening_brace_count = trimmed_line.chars().filter(|character| *character == '{').count();
            let closing_brace_count = trimmed_line.chars().filter(|character| *character == '}').count();
            let net_brace_change =
                i32::try_from(opening_brace_count).unwrap_or(i32::MAX) - i32::try_from(closing_brace_count).unwrap_or(i32::MAX);

            if net_brace_change > 0 {
                indentation_depth += net_brace_change;

                continue;
            }

            if net_brace_change < 0 && !trimmed_line.starts_with('}') {
                indentation_depth = (indentation_depth + net_brace_change).max(0);
            }
        }

        let mut formatted_text = formatted_lines.join("\n");

        if !formatted_text.ends_with('\n') {
            formatted_text.push('\n');
        }

        formatted_text
    }
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
