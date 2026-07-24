use superwire_dsl::parse_workflow;

use lsp_types::Range;

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
        self.range_for_byte_offsets(0, self.text.len())
            .unwrap_or_else(|| self.line_index.zero_range())
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
