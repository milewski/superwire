use std::collections::HashMap;

pub(super) struct CommentPreserver<'source> {
    source_text: &'source str,
    formatted_without_comments: String,
}

impl<'source> CommentPreserver<'source> {
    pub(super) fn new(source_text: &'source str, formatted_without_comments: String) -> Self {
        Self {
            source_text,
            formatted_without_comments,
        }
    }

    pub(super) fn with_preserved_comments(self) -> String {
        let source_line_analyses = SourceLineAnalyzer::new(self.source_text).analyze();

        if !source_line_analyses.iter().any(SourceLineAnalysis::has_comment) {
            return self.formatted_without_comments;
        }

        let mut formatted_lines = self.formatted_without_comments.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

        let source_code_signature_lines = SourceCodeSignatureLine::collect(&source_line_analyses);
        let formatted_code_signature_lines = FormattedCodeSignatureLine::collect(&formatted_lines);
        let source_to_formatted_map = map_source_lines_to_formatted_lines(&source_code_signature_lines, &formatted_code_signature_lines);

        apply_inline_comments(&source_line_analyses, &source_to_formatted_map, &mut formatted_lines);
        apply_standalone_comments(&source_line_analyses, &source_to_formatted_map, &mut formatted_lines);

        let mut formatted_with_comments = formatted_lines.join("\n");

        if self.formatted_without_comments.ends_with('\n') {
            formatted_with_comments.push('\n');
        }

        formatted_with_comments
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommentKind {
    Inline,
    Standalone,
}

#[derive(Clone, Debug)]
struct CommentFragment {
    text: String,
    comment_kind: CommentKind,
}

#[derive(Clone, Debug)]
struct SourceLineAnalysis {
    line_number: usize,
    code_text: String,
    comment: Option<CommentFragment>,
    is_within_multiline_string: bool,
}

impl SourceLineAnalysis {
    fn has_comment(&self) -> bool {
        self.comment.is_some()
    }

    fn code_signature(&self) -> Option<String> {
        if self.is_within_multiline_string {
            return None;
        }

        line_signature(&self.code_text)
    }

    fn is_blank_line(&self) -> bool {
        self.comment.is_none() && self.code_text.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringScanState {
    Normal,
    QuotedString,
    MultilineString,
}

struct SourceLineAnalyzer<'source> {
    source_text: &'source str,
}

impl<'source> SourceLineAnalyzer<'source> {
    fn new(source_text: &'source str) -> Self {
        Self { source_text }
    }

    fn analyze(&self) -> Vec<SourceLineAnalysis> {
        let mut source_line_analyses = Vec::new();
        let mut string_scan_state = StringScanState::Normal;

        for (line_index, source_line) in self.source_text.lines().enumerate() {
            let starts_inside_multiline_string = string_scan_state == StringScanState::MultilineString;
            let comment_start_byte_index = find_comment_start_byte_index(source_line, &mut string_scan_state);

            let (code_text, comment) = if let Some(comment_start) = comment_start_byte_index {
                let code_text = source_line[..comment_start].to_owned();
                let comment_text = source_line[comment_start..].to_owned();

                if comment_text.trim_start().starts_with("///") {
                    source_line_analyses.push(SourceLineAnalysis {
                        line_number: line_index + 1,
                        code_text,
                        comment: None,
                        is_within_multiline_string: starts_inside_multiline_string,
                    });

                    continue;
                }

                let comment_kind = if code_text.trim().is_empty() {
                    CommentKind::Standalone
                } else {
                    CommentKind::Inline
                };

                (
                    code_text,
                    Some(CommentFragment {
                        text: comment_text,
                        comment_kind,
                    }),
                )
            } else {
                (source_line.to_owned(), None)
            };

            source_line_analyses.push(SourceLineAnalysis {
                line_number: line_index + 1,
                code_text,
                comment,
                is_within_multiline_string: starts_inside_multiline_string,
            });
        }

        source_line_analyses
    }
}

fn find_comment_start_byte_index(source_line: &str, string_scan_state: &mut StringScanState) -> Option<usize> {
    let mut byte_index = 0;

    while byte_index < source_line.len() {
        let remaining_source = &source_line[byte_index..];

        if *string_scan_state == StringScanState::Normal && remaining_source.starts_with("\"\"\"") {
            *string_scan_state = StringScanState::MultilineString;
            byte_index += 3;
            continue;
        }

        if *string_scan_state == StringScanState::MultilineString && remaining_source.starts_with("\"\"\"") {
            *string_scan_state = StringScanState::Normal;
            byte_index += 3;
            continue;
        }

        if *string_scan_state == StringScanState::Normal && remaining_source.starts_with("//") {
            return Some(byte_index);
        }

        let current_character = remaining_source
            .chars()
            .next()
            .expect("remaining source should include a character");

        match string_scan_state {
            StringScanState::Normal => {
                if current_character == '"' {
                    *string_scan_state = StringScanState::QuotedString;
                }
            }
            StringScanState::QuotedString => {
                if current_character == '\\' {
                    byte_index += current_character.len_utf8();

                    if byte_index < source_line.len() {
                        let escaped_character = source_line[byte_index..].chars().next().expect("escaped character should exist");

                        byte_index += escaped_character.len_utf8();
                    }

                    continue;
                }

                if current_character == '"' {
                    *string_scan_state = StringScanState::Normal;
                }
            }
            StringScanState::MultilineString => {}
        }

        byte_index += current_character.len_utf8();
    }

    if *string_scan_state == StringScanState::QuotedString {
        *string_scan_state = StringScanState::Normal;
    }

    None
}

#[derive(Clone, Debug)]
struct SourceCodeSignatureLine {
    source_line_number: usize,
    signature: String,
}

impl SourceCodeSignatureLine {
    fn collect(source_line_analyses: &[SourceLineAnalysis]) -> Vec<Self> {
        let mut source_code_signature_lines = Vec::new();

        for source_line_analysis in source_line_analyses {
            let Some(signature) = source_line_analysis.code_signature() else {
                continue;
            };

            source_code_signature_lines.push(Self {
                source_line_number: source_line_analysis.line_number,
                signature,
            });
        }

        source_code_signature_lines
    }
}

#[derive(Clone, Debug)]
struct FormattedCodeSignatureLine {
    formatted_line_index: usize,
    signature: String,
}

impl FormattedCodeSignatureLine {
    fn collect(formatted_lines: &[String]) -> Vec<Self> {
        let mut formatted_code_signature_lines = Vec::new();
        let mut is_inside_multiline_string = false;

        for (line_index, line_text) in formatted_lines.iter().enumerate() {
            let is_current_line_within_multiline = is_inside_multiline_string;
            is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);

            if is_current_line_within_multiline || line_text.trim() == "\"\"\"" {
                continue;
            }

            let Some(signature) = line_signature(line_text) else {
                continue;
            };

            formatted_code_signature_lines.push(Self {
                formatted_line_index: line_index,
                signature,
            });
        }

        formatted_code_signature_lines
    }
}

fn line_signature(line_text: &str) -> Option<String> {
    let compact_signature = line_text.chars().filter(|character| !character.is_whitespace()).collect::<String>();
    let normalized_signature = compact_signature.trim_end_matches(',').to_owned();

    if normalized_signature.is_empty() {
        return None;
    }

    Some(normalized_signature)
}

fn map_source_lines_to_formatted_lines(
    source_code_signature_lines: &[SourceCodeSignatureLine],
    formatted_code_signature_lines: &[FormattedCodeSignatureLine],
) -> HashMap<usize, usize> {
    let mut source_to_formatted_map = HashMap::new();
    let mut formatted_cursor = 0_usize;

    for source_code_signature_line in source_code_signature_lines {
        let relative_match_index = formatted_code_signature_lines[formatted_cursor..]
            .iter()
            .position(|formatted_code_signature_line| formatted_code_signature_line.signature == source_code_signature_line.signature);

        let Some(relative_match_index) = relative_match_index else {
            continue;
        };

        let absolute_match_index = formatted_cursor + relative_match_index;
        let formatted_code_signature_line = &formatted_code_signature_lines[absolute_match_index];

        source_to_formatted_map.insert(
            source_code_signature_line.source_line_number,
            formatted_code_signature_line.formatted_line_index,
        );

        formatted_cursor = absolute_match_index + 1;
    }

    source_to_formatted_map
}

fn apply_inline_comments(
    source_line_analyses: &[SourceLineAnalysis],
    source_to_formatted_map: &HashMap<usize, usize>,
    formatted_lines: &mut [String],
) {
    for source_line_analysis in source_line_analyses {
        let Some(comment) = &source_line_analysis.comment else {
            continue;
        };

        if comment.comment_kind != CommentKind::Inline {
            continue;
        }

        let Some(formatted_line_index) = source_to_formatted_map.get(&source_line_analysis.line_number) else {
            continue;
        };

        let Some(formatted_line) = formatted_lines.get_mut(*formatted_line_index) else {
            continue;
        };

        if formatted_line.trim().is_empty() {
            comment.text.trim_start().clone_into(formatted_line);
            continue;
        }

        formatted_line.push(' ');
        formatted_line.push_str(comment.text.trim_start());
    }
}

#[derive(Clone, Debug)]
struct StandaloneCommentInsertion {
    source_line_number: usize,
    target_formatted_line_index: usize,
    insert_after_target: bool,
    preserve_blank_line_before: bool,
    preserve_blank_line_after: bool,
    comment_text: String,
}

fn apply_standalone_comments(
    source_line_analyses: &[SourceLineAnalysis],
    source_to_formatted_map: &HashMap<usize, usize>,
    formatted_lines: &mut Vec<String>,
) {
    let mut standalone_comment_insertions = Vec::new();
    let source_line_count = source_line_analyses.len();

    for (analysis_index, source_line_analysis) in source_line_analyses.iter().enumerate() {
        let Some(comment) = &source_line_analysis.comment else {
            continue;
        };

        if comment.comment_kind != CommentKind::Standalone {
            continue;
        }

        let next_mapped_line =
            find_next_mapped_formatted_line(source_line_analysis.line_number, source_line_count, source_to_formatted_map);
        let previous_mapped_line = find_previous_mapped_formatted_line(source_line_analysis.line_number, source_to_formatted_map);

        let (target_formatted_line_index, insert_after_target) = if let Some(next_line) = next_mapped_line {
            (next_line, false)
        } else if let Some(previous_line) = previous_mapped_line {
            if let Some(next_non_empty_line) =
                find_first_non_empty_formatted_line_outside_multiline_strings_after(previous_line, formatted_lines)
            {
                (next_non_empty_line, false)
            } else {
                (previous_line, true)
            }
        } else {
            (0, false)
        };

        let indentation_source_line = formatted_lines.get(target_formatted_line_index);

        let indentation = indentation_source_line
            .map(|line_text| leading_whitespace(line_text.as_str()))
            .unwrap_or_default();
        let preserve_blank_line_before = source_line_analyses
            .get(analysis_index.saturating_sub(1))
            .is_some_and(SourceLineAnalysis::is_blank_line);
        let preserve_blank_line_after = source_line_analyses
            .get(analysis_index + 1)
            .is_some_and(SourceLineAnalysis::is_blank_line);

        standalone_comment_insertions.push(StandaloneCommentInsertion {
            source_line_number: source_line_analysis.line_number,
            target_formatted_line_index,
            insert_after_target,
            preserve_blank_line_before,
            preserve_blank_line_after,
            comment_text: format!("{indentation}{}", comment.text.trim_start()),
        });
    }

    standalone_comment_insertions.sort_by_key(|comment_insertion| {
        (
            comment_insertion.target_formatted_line_index,
            comment_insertion.insert_after_target,
            comment_insertion.source_line_number,
        )
    });

    let mut insertion_offset = 0_usize;

    for standalone_comment_insertion in standalone_comment_insertions {
        let base_insertion_index = if standalone_comment_insertion.insert_after_target {
            standalone_comment_insertion.target_formatted_line_index.saturating_add(1)
        } else {
            standalone_comment_insertion.target_formatted_line_index
        };

        let mut insertion_index = base_insertion_index.saturating_add(insertion_offset).min(formatted_lines.len());

        let should_preserve_or_insert_blank_line_before = standalone_comment_insertion.preserve_blank_line_before
            || should_insert_visual_separator_before_comment(insertion_index, formatted_lines);

        if should_preserve_or_insert_blank_line_before && !has_blank_line_before_index(insertion_index, formatted_lines) {
            formatted_lines.insert(insertion_index, String::new());
            insertion_offset += 1;
            insertion_index += 1;
        }

        formatted_lines.insert(insertion_index, standalone_comment_insertion.comment_text);
        insertion_offset += 1;
        insertion_index += 1;

        if standalone_comment_insertion.preserve_blank_line_after && !has_blank_line_at_index(insertion_index, formatted_lines) {
            formatted_lines.insert(insertion_index, String::new());
            insertion_offset += 1;
        }
    }
}

fn has_blank_line_before_index(insertion_index: usize, formatted_lines: &[String]) -> bool {
    if insertion_index == 0 {
        return false;
    }

    formatted_lines
        .get(insertion_index.saturating_sub(1))
        .is_some_and(|line_text| line_text.trim().is_empty())
}

fn has_blank_line_at_index(insertion_index: usize, formatted_lines: &[String]) -> bool {
    formatted_lines
        .get(insertion_index)
        .is_some_and(|line_text| line_text.trim().is_empty())
}

fn should_insert_visual_separator_before_comment(insertion_index: usize, formatted_lines: &[String]) -> bool {
    let mut previous_line_index = insertion_index;

    while previous_line_index > 0 {
        previous_line_index = previous_line_index.saturating_sub(1);

        let Some(previous_line_text) = formatted_lines.get(previous_line_index) else {
            continue;
        };

        if previous_line_text.trim().is_empty() {
            continue;
        }

        let previous_line_without_indent = previous_line_text.trim_start();

        if previous_line_without_indent.starts_with("//") {
            return false;
        }

        let previous_line_without_trailing_whitespace = previous_line_text.trim_end();

        if previous_line_without_trailing_whitespace.ends_with('{') || previous_line_without_trailing_whitespace.ends_with('[') {
            return false;
        }

        return true;
    }

    false
}

fn find_next_mapped_formatted_line(
    source_line_number: usize,
    source_line_count: usize,
    source_to_formatted_map: &HashMap<usize, usize>,
) -> Option<usize> {
    for line_number in source_line_number + 1..=source_line_count {
        let Some(formatted_line_index) = source_to_formatted_map.get(&line_number) else {
            continue;
        };

        return Some(*formatted_line_index);
    }

    None
}

fn find_previous_mapped_formatted_line(source_line_number: usize, source_to_formatted_map: &HashMap<usize, usize>) -> Option<usize> {
    if source_line_number <= 1 {
        return None;
    }

    for line_number in (1..source_line_number).rev() {
        let Some(formatted_line_index) = source_to_formatted_map.get(&line_number) else {
            continue;
        };

        return Some(*formatted_line_index);
    }

    None
}

fn find_first_non_empty_formatted_line_outside_multiline_strings_after(
    start_line_index: usize,
    formatted_lines: &[String],
) -> Option<usize> {
    let mut is_inside_multiline_string = false;

    for line_text in formatted_lines.iter().take(start_line_index.saturating_add(1)) {
        is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);
    }

    let first_candidate_index = start_line_index.saturating_add(1);

    for line_index in first_candidate_index..formatted_lines.len() {
        let Some(line_text) = formatted_lines.get(line_index) else {
            continue;
        };

        is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);

        if is_inside_multiline_string || line_text.trim() == "\"\"\"" {
            continue;
        }

        if line_text.trim().is_empty() {
            continue;
        }

        return Some(line_index);
    }

    None
}

fn update_multiline_string_state(current_state: bool, line_text: &str) -> bool {
    let triple_quote_occurrences = line_text.matches("\"\"\"").count();

    if triple_quote_occurrences.is_multiple_of(2) {
        return current_state;
    }

    !current_state
}

fn leading_whitespace(line_text: &str) -> String {
    line_text
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<String>()
}
