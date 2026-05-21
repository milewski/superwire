use crate::diagnostic::DiagnosticCode;
use crate::dsl::{format_workflow_source, DslFormatError};
use serde_json::Value;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

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
pub enum WorkflowSource {
    Inline(String),
    File(PathBuf),
}

#[derive(Debug)]
pub enum WorkflowSourceReadError {
    Io(std::io::Error),
    Format(DslFormatError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedDiagnostic {
    pub code: DiagnosticCode,
    pub message_contains: Option<String>,
    pub span_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedOutput {
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedProviderRequest {
    pub provider: String,
    pub model: Option<String>,
    pub prompt_contains: Option<String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedMcpRequest {
    pub server: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedEventKind {
    WorkflowStarted,
    WorkflowPlanned,
    WorkflowCompleted,
    WorkflowFailed,
    AgentStarted,
    AgentCompleted,
    ToolCallStarted,
    ToolCallCompleted,
    ToolCallFailed,
    McpCallStarted,
    McpCallCompleted,
    McpCallFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedEvent {
    pub kind: ExpectedEventKind,
    pub agent_name: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedCompletion {
    pub label: String,
    pub kind: Option<ExpectedCompletionKind>,
    pub detail_contains: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedCompletionKind {
    Keyword,
    Function,
    Field,
    Variable,
    Value,
    Module,
    Struct,
    Enum,
    Property,
    Text,
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
    pub fn into_source(self) -> String {
        self.source_text
    }

    #[must_use]
    pub fn cursor_position(&self) -> InlineCursorPosition {
        self.cursor_position
    }
}

impl WorkflowSource {
    #[must_use]
    pub fn inline(source_text: impl Into<String>) -> Self {
        Self::Inline(source_text.into())
    }

    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    #[must_use]
    pub fn fixture(root: impl AsRef<Path>, relative_path: impl AsRef<Path>) -> Self {
        Self::File(root.as_ref().join(relative_path))
    }

    pub fn read(&self) -> Result<String, WorkflowSourceReadError> {
        match self {
            Self::Inline(source_text) => Ok(source_text.clone()),
            Self::File(path) => std::fs::read_to_string(path).map_err(WorkflowSourceReadError::Io),
        }
    }

    pub fn read_formatted(&self) -> Result<String, WorkflowSourceReadError> {
        let source_text = self.read()?;
        format_workflow_source(&source_text).map_err(WorkflowSourceReadError::Format)
    }
}

impl fmt::Display for WorkflowSourceReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(read_error) => write!(formatter, "{read_error}"),
            Self::Format(format_error) => write!(formatter, "{format_error}"),
        }
    }
}

impl std::error::Error for WorkflowSourceReadError {}

impl ExpectedDiagnostic {
    #[must_use]
    pub fn code(code: DiagnosticCode) -> Self {
        Self {
            code,
            message_contains: None,
            span_text: None,
        }
    }

    #[must_use]
    pub fn message_contains(mut self, message_contains: impl Into<String>) -> Self {
        self.message_contains = Some(message_contains.into());
        self
    }

    #[must_use]
    pub fn span_text(mut self, span_text: impl Into<String>) -> Self {
        self.span_text = Some(span_text.into());
        self
    }
}

impl ExpectedOutput {
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl ExpectedProviderRequest {
    #[must_use]
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: None,
            prompt_contains: None,
            tools: Vec::new(),
        }
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn prompt_contains(mut self, prompt_contains: impl Into<String>) -> Self {
        self.prompt_contains = Some(prompt_contains.into());
        self
    }

    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools = tools.into_iter().map(Into::into).collect();
        self
    }
}

impl ExpectedMcpRequest {
    #[must_use]
    pub fn new(server: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            method: method.into(),
            params: None,
        }
    }

    #[must_use]
    pub fn params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }
}

impl ExpectedEvent {
    #[must_use]
    pub fn new(kind: ExpectedEventKind) -> Self {
        Self {
            kind,
            agent_name: None,
            tool_name: None,
        }
    }

    #[must_use]
    pub fn agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }

    #[must_use]
    pub fn tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }
}

impl ExpectedCompletion {
    #[must_use]
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: None,
            detail_contains: None,
        }
    }

    #[must_use]
    pub fn kind(mut self, kind: ExpectedCompletionKind) -> Self {
        self.kind = Some(kind);
        self
    }

    #[must_use]
    pub fn detail_contains(mut self, detail_contains: impl Into<String>) -> Self {
        self.detail_contains = Some(detail_contains.into());
        self
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
        assert!(
            self.expected == self.actual,
            "snapshot `{}` did not match\n{}",
            self.name,
            stable_text_diff(&self.expected, &self.actual)
        );
    }
}

#[must_use]
pub fn empty_object_schema() -> Value {
    serde_json::to_value(schemars::json_schema!({
        "type": "object",
        "properties": {},
        "required": [],
        "additionalProperties": false,
    }))
    .expect("empty object schema should serialize")
}

#[must_use]
pub fn schema_for_type<Type>() -> Value
where
    Type: schemars::JsonSchema,
{
    let mut schema = serde_json::to_value(schemars::schema_for!(Type)).expect("test schema should serialize");

    if let Some(schema_object) = schema.as_object_mut() {
        schema_object.remove("$schema");
        schema_object.remove("title");
    }

    schema
}

#[must_use]
pub fn stable_text_diff(expected: &str, actual: &str) -> String {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let line_count = expected_lines.len().max(actual_lines.len());
    let mut difference_text = String::new();

    for line_index in 0..line_count {
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

#[cfg(test)]
mod tests {
    use super::{empty_object_schema, stable_text_diff, WorkflowSource, WorkflowSourceTemplate};
    use serde_json::json;

    #[test]
    fn inline_cursor_layout_normalizes_cursor_before_block_close() {
        let source_template = WorkflowSourceTemplate::from_inline(crate::workflow_source! {
            agent worker { <cursor> }
        });
        let source_with_cursor = source_template.with_cursor();

        assert_eq!(source_with_cursor.cursor_position().line, 1);
        assert_eq!(source_with_cursor.cursor_position().character, 0);
        assert_eq!(source_with_cursor.source(), "agent worker { \n\n }");
    }

    #[test]
    fn stable_text_diff_reports_changed_lines() {
        let difference_text = stable_text_diff("alpha\nbeta", "alpha\ngamma");

        assert!(difference_text.contains("line 2:"));
        assert!(difference_text.contains("expected: beta"));
        assert!(difference_text.contains("actual:   gamma"));
    }

    #[test]
    fn inline_workflow_source_reads_formatted_source() {
        let workflow_source = WorkflowSource::inline(crate::workflow_source! {
            input {
                project_id:number
            }
        });

        let formatted_source = workflow_source.read_formatted().expect("inline workflow source should format");

        assert!(formatted_source.contains("project_id: number"));
    }

    #[test]
    fn empty_object_schema_returns_stable_schema_value() {
        let schema = empty_object_schema();

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false,
            })
        );
    }
}
