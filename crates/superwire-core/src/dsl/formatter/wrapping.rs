use super::DslFormatter;

const MAX_LINE_WIDTH: usize = 120;
const WRAP_WIDTH_BUFFER: usize = 12;

impl DslFormatter {
    pub(super) fn push_declaration_block_start(&mut self, header: &str) {
        self.push_line(&format!("{header} {{"));
        self.indentation_depth += 1;
    }

    pub(super) fn push_declaration_block_end(&mut self) {
        self.indentation_depth -= 1;
        self.push_line("}");
    }

    pub(super) fn push_indent(&mut self) {
        for _ in 0..self.indentation_depth {
            self.output.push_str("    ");
        }
    }

    pub(super) fn push_line(&mut self, line: &str) {
        self.push_indent();
        self.output.push_str(line);
        self.push_newline();
    }

    pub(super) fn push_newline(&mut self) {
        self.output.push('\n');
    }

    pub(super) fn push_multiline_string_block(&mut self, escaped_multiline_contents: &str) {
        let normalized_multiline_lines = Self::normalize_multiline_string_lines(escaped_multiline_contents);
        let wrapped_multiline_lines = self.wrap_multiline_lines_to_width(&normalized_multiline_lines);

        self.output.push_str("\"\"\"");
        self.push_newline();
        self.indentation_depth += 1;

        for multiline_content_line in wrapped_multiline_lines {
            self.push_indent();
            self.output.push_str(&multiline_content_line);
            self.push_newline();
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push_str("\"\"\"");
    }

    pub(super) fn push_multiline_string_block_from_lines(&mut self, multiline_content_lines: &[String]) {
        self.output.push_str("\"\"\"");
        self.push_newline();
        self.indentation_depth += 1;

        for multiline_content_line in multiline_content_lines {
            self.push_indent();
            self.output.push_str(multiline_content_line);
            self.push_newline();
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push_str("\"\"\"");
    }

    pub(super) fn wrap_multiline_lines_to_width(&self, multiline_content_lines: &[String]) -> Vec<String> {
        let line_width_limit = self.multiline_content_width_limit();
        let mut wrapped_multiline_lines = Vec::new();

        for multiline_content_line in multiline_content_lines {
            if multiline_content_line.trim().is_empty() {
                wrapped_multiline_lines.push(String::new());
                continue;
            }

            wrapped_multiline_lines.extend(wrap_text_line_by_words(multiline_content_line, line_width_limit));
        }

        wrapped_multiline_lines
    }

    pub(super) fn can_fit_inline_text(&self, inline_text: &str) -> bool {
        !inline_text.contains('\n') && self.current_line_width() + inline_text.chars().count() <= MAX_LINE_WIDTH
    }

    pub(super) fn multiline_content_width_limit(&self) -> usize {
        MAX_LINE_WIDTH.saturating_sub((self.indentation_depth + 1) * 4).max(20)
    }

    pub(super) fn current_line_width(&self) -> usize {
        self.output.rsplit('\n').next().map_or(0, |line_text| line_text.chars().count())
    }

    pub(super) fn wrap_multiline_string_value(&self, raw_string: &str) -> Vec<String> {
        wrap_text_line_by_words(raw_string.trim(), self.multiline_content_width_limit())
            .into_iter()
            .map(|wrapped_line| escape_multiline_string_text(&wrapped_line))
            .collect::<Vec<_>>()
    }

    pub(super) fn normalize_multiline_string_lines(multiline_contents: &str) -> Vec<String> {
        let mut content_lines = multiline_contents.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();

        while content_lines.first().is_some_and(|line_text| line_text.trim().is_empty()) {
            let _ = content_lines.remove(0);
        }

        while content_lines.last().is_some_and(|line_text| line_text.trim().is_empty()) {
            let _ = content_lines.pop();
        }

        if content_lines.is_empty() {
            return content_lines;
        }

        let minimum_indentation = content_lines
            .iter()
            .filter(|line_text| !line_text.trim().is_empty())
            .map(|line_text| line_text.chars().take_while(|character| character.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        content_lines
            .into_iter()
            .map(|line_text| {
                if line_text.trim().is_empty() {
                    return String::new();
                }

                line_text.chars().skip(minimum_indentation).collect::<String>()
            })
            .collect::<Vec<_>>()
    }
}

fn find_wrap_split_index(text: &str, width_limit: usize) -> Option<usize> {
    let mut byte_index = 0_usize;
    let mut character_index = 0_usize;
    let mut last_whitespace_character_index = None;
    let mut is_inside_interpolation = false;

    while byte_index < text.len() {
        let remaining_text = &text[byte_index..];

        if !is_inside_interpolation && remaining_text.starts_with("{{") {
            is_inside_interpolation = true;
            byte_index += 2;
            character_index += 2;
            continue;
        }

        if is_inside_interpolation && remaining_text.starts_with("}}") {
            is_inside_interpolation = false;
            byte_index += 2;
            character_index += 2;
            continue;
        }

        if character_index >= width_limit {
            break;
        }

        let Some(current_character) = remaining_text.chars().next() else {
            break;
        };

        if current_character.is_whitespace() && !is_inside_interpolation {
            last_whitespace_character_index = Some(character_index);
        }

        byte_index += current_character.len_utf8();
        character_index += 1;
    }

    last_whitespace_character_index
}

pub(super) fn wrap_text_line_by_words(text_line: &str, width_limit: usize) -> Vec<String> {
    let trimmed_text_line = text_line.trim();

    if trimmed_text_line.is_empty() {
        return vec![String::new()];
    }

    let mut wrapped_lines = Vec::new();
    let mut remaining_text = trimmed_text_line.to_owned();
    let width_limit_with_buffer = width_limit.saturating_add(WRAP_WIDTH_BUFFER);

    while remaining_text.chars().count() > width_limit_with_buffer {
        let split_character_index =
            find_wrap_split_index(&remaining_text, width_limit).or_else(|| find_wrap_split_index(&remaining_text, width_limit_with_buffer));
        let Some(split_character_index) = split_character_index else {
            break;
        };

        if split_character_index == 0 {
            break;
        }

        let wrapped_line = remaining_text
            .chars()
            .take(split_character_index)
            .collect::<String>()
            .trim_end()
            .to_owned();

        if wrapped_line.is_empty() {
            break;
        }

        wrapped_lines.push(wrapped_line);

        let wrapped_remainder = remaining_text
            .chars()
            .skip(split_character_index)
            .collect::<String>()
            .trim_start()
            .to_owned();

        wrapped_remainder.clone_into(&mut remaining_text);
    }

    wrapped_lines.push(remaining_text.trim_end().to_owned());
    wrapped_lines
}

pub(super) fn render_expression_string_literal(raw_string: &str) -> String {
    if raw_string.contains('\n') {
        return format!("\"\"\"{}\"\"\"", escape_multiline_string_text(raw_string));
    }

    format!("\"{}\"", escape_quoted_string_text(raw_string))
}

pub(super) fn render_plain_string_literal(raw_string: &str) -> String {
    if raw_string.contains('\n') {
        return format!("\"\"\"{}\"\"\"", escape_multiline_plain_string_text(raw_string));
    }

    format!("\"{}\"", escape_plain_string_text(raw_string))
}

pub(super) fn render_object_field_name(field_name: &str) -> String {
    if is_identifier_name(field_name) {
        return field_name.to_string();
    }

    render_plain_string_literal(field_name)
}

fn is_identifier_name(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };

    if !first_character.is_ascii_alphabetic() && first_character != '_' {
        return false;
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(super) fn escape_quoted_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '"' => escaped_string.push_str("\\\""),
            '\n' => escaped_string.push_str("\\n"),
            '\r' => escaped_string.push_str("\\r"),
            '\t' => escaped_string.push_str("\\t"),
            '{' => escaped_string.push_str("\\{"),
            '}' => escaped_string.push_str("\\}"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string
}

pub(super) fn escape_plain_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '"' => escaped_string.push_str("\\\""),
            '\n' => escaped_string.push_str("\\n"),
            '\r' => escaped_string.push_str("\\r"),
            '\t' => escaped_string.push_str("\\t"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string
}

pub(super) fn escape_multiline_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '{' => escaped_string.push_str("\\{"),
            '}' => escaped_string.push_str("\\}"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string.replace("\"\"\"", "\\\"\\\"\\\"")
}

pub(super) fn escape_multiline_plain_string_text(raw_string: &str) -> String {
    raw_string.replace("\"\"\"", "\\\"\\\"\\\"")
}
