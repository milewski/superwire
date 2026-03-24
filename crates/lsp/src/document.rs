use std::sync::OnceLock;

use regex::Regex;

use crate::protocol::Position;

#[derive(Debug, Clone)]
pub struct DocumentIndex {
    text: String,
}

impl DocumentIndex {
    #[must_use]
    pub fn new(text: String) -> Self {
        Self { text }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn line_prefix(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let character_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().take(character_index).collect())
    }

    #[must_use]
    pub fn symbol_at(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();

        if line_characters.is_empty() {
            return None;
        }

        let mut cursor_index = usize::min(position.character as usize, line_characters.len().saturating_sub(1));

        if !is_symbol_character(line_characters[cursor_index]) {
            if cursor_index == 0 || !is_symbol_character(line_characters[cursor_index - 1]) {
                return None;
            }

            cursor_index -= 1;
        }

        let mut start_index = cursor_index;

        while start_index > 0 && is_symbol_character(line_characters[start_index - 1]) {
            start_index -= 1;
        }

        let mut end_index = cursor_index + 1;

        while end_index < line_characters.len() && is_symbol_character(line_characters[end_index]) {
            end_index += 1;
        }

        Some(line_characters[start_index..end_index].iter().collect())
    }

    #[must_use]
    pub fn schema_names(&self) -> Vec<String> {
        collect_named_matches(self.text(), schema_name_regex())
    }

    #[must_use]
    pub fn agent_names(&self) -> Vec<String> {
        collect_named_matches(self.text(), agent_name_regex())
    }

    #[must_use]
    pub fn provider_names(&self) -> Vec<String> {
        collect_named_matches(self.text(), provider_name_regex())
    }

    #[must_use]
    pub fn input_fields(&self) -> Vec<String> {
        collect_block_fields(self.text(), "input")
    }

    #[must_use]
    pub fn secret_fields(&self) -> Vec<String> {
        collect_block_fields(self.text(), "secrets")
    }
}

fn is_symbol_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.'
}

fn collect_named_matches(source_text: &str, pattern: &Regex) -> Vec<String> {
    let mut matches = Vec::new();

    for capture in pattern.captures_iter(source_text) {
        if let Some(name_match) = capture.get(1) {
            let candidate_name = name_match.as_str().to_owned();

            if !matches.contains(&candidate_name) {
                matches.push(candidate_name);
            }
        }
    }

    matches
}

fn collect_block_fields(source_text: &str, block_name: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut inside_block = false;
    let mut brace_depth = 0_isize;

    for source_line in source_text.lines() {
        let trimmed_line = source_line.trim();

        if !inside_block {
            let starts_named_block = trimmed_line.starts_with(block_name) && trimmed_line[block_name.len()..].trim_start().starts_with('{');

            if starts_named_block {
                inside_block = true;
                brace_depth = 1;
            }

            continue;
        }

        if brace_depth == 1 {
            if let Some(capture) = field_name_regex().captures(trimmed_line) {
                if let Some(name_match) = capture.get(1) {
                    let field_name = name_match.as_str().to_owned();

                    if !fields.contains(&field_name) {
                        fields.push(field_name);
                    }
                }
            }
        }

        let opening_braces = isize::try_from(count_occurrences(trimmed_line, '{')).expect("opening brace count must fit in isize");
        let closing_braces = isize::try_from(count_occurrences(trimmed_line, '}')).expect("closing brace count must fit in isize");

        brace_depth += opening_braces;
        brace_depth -= closing_braces;

        if brace_depth <= 0 {
            inside_block = false;
            brace_depth = 0;
        }
    }

    fields
}

fn count_occurrences(source_text: &str, needle: char) -> usize {
    source_text.chars().filter(|character| *character == needle).count()
}

fn schema_name_regex() -> &'static Regex {
    static SCHEMA_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

    SCHEMA_NAME_REGEX.get_or_init(|| Regex::new(r"(?m)^\s*schema\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("schema name regex must compile"))
}

fn agent_name_regex() -> &'static Regex {
    static AGENT_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

    AGENT_NAME_REGEX.get_or_init(|| Regex::new(r"(?m)^\s*agent\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("agent name regex must compile"))
}

fn provider_name_regex() -> &'static Regex {
    static PROVIDER_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

    PROVIDER_NAME_REGEX
        .get_or_init(|| Regex::new(r"(?m)^\s*provider\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("provider name regex must compile"))
}

fn field_name_regex() -> &'static Regex {
    static FIELD_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

    FIELD_NAME_REGEX.get_or_init(|| Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*:").expect("field name regex must compile"))
}
