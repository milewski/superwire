use super::{CompletionSuggestion, DocumentDiagnostic, DocumentState, Position, TypeExpression};
use crate::diagnostic_code::DiagnosticCode;
use lsp_types::CompletionItemKind;
use superwire_core::dsl::{
    AgentExpressionPropertyName, BuiltinFunctionName, DeclarationKeyword, ForClauseKeyword, McpCallOperation, ReferenceKeyword,
    SingletonDeclarationKind, ToolCallKeyword,
};
use superwire_core::semantic::InferenceSetting;

macro_rules! inline_completion_suggestions {
    ($($workflow_tokens:tt)*) => {{
        completion_suggestions_from_template(stringify!($($workflow_tokens)*))
    }};
}

macro_rules! inline_document_template {
    ($($workflow_tokens:tt)*) => {{
        stringify!($($workflow_tokens)*)
    }};
}

macro_rules! inline_diagnostics {
    ($($workflow_tokens:tt)*) => {{
        diagnostics_from_template(stringify!($($workflow_tokens)*))
    }};
}

macro_rules! assert_completion_contains_labels {
    ($completion_suggestions:expr, $($expected_label:expr),+ $(,)?) => {{
        let available_labels = completion_label_set($completion_suggestions);

        $(
            let expected_label = expected_completion_label($expected_label);

            assert!(
                available_labels.contains(expected_label),
                "expected completion label `{expected_label}`; available labels: {:?}",
                available_labels
            );
        )+
    }};
}

macro_rules! assert_completion_contains_all_inference_settings {
    ($completion_suggestions:expr) => {{
        assert_completion_contains_label_groups!($completion_suggestions, InferenceSetting);
    }};
}

macro_rules! assert_completion_contains_label_groups {
    ($completion_suggestions:expr, $($label_group:ident),+ $(,)?) => {{
        let available_labels = completion_label_set($completion_suggestions);

        $(
            assert_completion_contains_label_group::<$label_group>(&available_labels);
        )+
    }};
}

macro_rules! assert_completion_contains {
    ($completion_suggestions:expr, $first_label:expr $(, $additional_label:expr)* $(,)?) => {{
        assert_completion_contains_labels!($completion_suggestions, $first_label $(, $additional_label)*);
    }};
}

macro_rules! assert_diagnostics_contain_codes {
    ($diagnostics:expr, $($expected_code:expr),+ $(,)?) => {{
        $(
            assert!(
                diagnostic_has_code($diagnostics, $expected_code),
                "expected diagnostic code `{:?}`; diagnostics: {:?}",
                $expected_code,
                $diagnostics
            );
        )+
    }};
}

macro_rules! assert_completion_excludes_labels {
    ($completion_suggestions:expr, $label_group:ident $(,)?) => {{
        assert_completion_excludes_label_group::<$label_group>($completion_suggestions);
    }};

    ($completion_suggestions:expr, $($unexpected_label:expr),+ $(,)?) => {{
        let available_labels = completion_label_set($completion_suggestions);

        $(
            let unexpected_label = expected_completion_label($unexpected_label);

            assert!(
                !available_labels.contains(unexpected_label),
                "unexpected completion label `{unexpected_label}`; available labels: {:?}",
                available_labels
            );
        )+
    }};
}

macro_rules! assert_completion_excludes_kind {
    ($completion_suggestions:expr, $completion_kind_pattern:pat) => {{
        assert!(
            !$completion_suggestions
                .iter()
                .any(|completion_suggestion| matches!(completion_suggestion.kind, $completion_kind_pattern)),
            "unexpected completion kind `{}`; suggestions: {:?}",
            stringify!($completion_kind_pattern),
            $completion_suggestions
                .iter()
                .map(|completion_suggestion| { (completion_suggestion.label.clone(), completion_suggestion.kind,) })
                .collect::<Vec<_>>()
        );
    }};
}

fn completion_label_set(completion_suggestions: &[CompletionSuggestion]) -> std::collections::HashSet<&str> {
    completion_suggestions
        .iter()
        .map(|completion_suggestion| completion_suggestion.label.as_str())
        .collect()
}

fn assert_completion_excludes_label_group<TLabelGroup>(completion_suggestions: &[CompletionSuggestion])
where
    TLabelGroup: CompletionLabelGroup,
{
    let available_labels = completion_label_set(completion_suggestions);

    for label_in_group in TLabelGroup::completion_labels() {
        assert!(
            !available_labels.contains(label_in_group),
            "unexpected completion label `{label_in_group}` from group; available labels: {available_labels:?}"
        );
    }
}

fn assert_completion_contains_label_group<TLabelGroup>(available_labels: &std::collections::HashSet<&str>)
where
    TLabelGroup: CompletionLabelGroup,
{
    for label_in_group in TLabelGroup::completion_labels() {
        assert!(
            available_labels.contains(label_in_group),
            "expected completion label `{label_in_group}` from group; available labels: {available_labels:?}"
        );
    }
}

fn diagnostic_has_code(diagnostics: &[DocumentDiagnostic], expected_code: DiagnosticCode) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.code == expected_code)
}

fn expected_completion_label<Label>(label_value: Label) -> &'static str
where
    Label: CompletionLabel,
{
    label_value.completion_label()
}

trait CompletionLabel {
    fn completion_label(self) -> &'static str;
}

trait CompletionLabelGroup {
    fn completion_labels() -> Vec<&'static str>;
}

impl CompletionLabel for &'static str {
    fn completion_label(self) -> &'static str {
        self
    }
}

impl CompletionLabel for InferenceSetting {
    fn completion_label(self) -> &'static str {
        self.key()
    }
}

impl CompletionLabelGroup for InferenceSetting {
    fn completion_labels() -> Vec<&'static str> {
        InferenceSetting::all().into_iter().map(InferenceSetting::key).collect()
    }
}

impl CompletionLabelGroup for BuiltinFunctionName {
    fn completion_labels() -> Vec<&'static str> {
        vec![Self::Context.as_str(), Self::Template.as_str(), Self::Compact.as_str()]
    }
}

impl CompletionLabelGroup for SingletonDeclarationKind {
    fn completion_labels() -> Vec<&'static str> {
        vec![Self::Input.as_str(), Self::Secrets.as_str(), Self::Output.as_str()]
    }
}

impl CompletionLabel for AgentExpressionPropertyName {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for BuiltinFunctionName {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for ToolCallKeyword {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for McpCallOperation {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for ReferenceKeyword {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for SingletonDeclarationKind {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for DeclarationKeyword {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for ForClauseKeyword {
    fn completion_label(self) -> &'static str {
        self.as_str()
    }
}

impl CompletionLabel for TypeExpression {
    fn completion_label(self) -> &'static str {
        match self {
            TypeExpression::String => "string",
            TypeExpression::Number => "number",
            TypeExpression::Float => "float",
            TypeExpression::Boolean => "boolean",
            TypeExpression::Null => "null",
            TypeExpression::AnyObject => "object",
            TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::Union(_) => {
                panic!("completion label is only defined for primitive TypeExpression variants")
            }
        }
    }
}

fn source_with_cursor(source_template: &str) -> (String, Position) {
    let normalized_template = normalize_inline_cursor_layout(source_template);

    source_without_cursor_normalization(&normalized_template)
}

fn source_without_cursor_normalization(source_template: &str) -> (String, Position) {
    let compact_cursor_marker = "<cursor>";

    let (cursor_marker, cursor_byte_offset) = if let Some(marker_offset) = source_template.find(compact_cursor_marker) {
        (compact_cursor_marker, marker_offset)
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

    let source_without_cursor = source_template.replacen(cursor_marker, "", 1);

    (source_without_cursor, Position { line, character })
}

fn completion_suggestions_from_template(source_template: &str) -> Vec<CompletionSuggestion> {
    let source_template = normalize_rust_doc_comment_tokens(source_template);
    let (source, cursor_position) = source_with_cursor(&source_template);

    completion_suggestions_from_source(source, cursor_position)
}

fn completion_suggestions_from_source(source: String, cursor_position: Position) -> Vec<CompletionSuggestion> {
    let document_state = DocumentState::new(source, None);

    document_state.completion_suggestions(cursor_position)
}

fn completion_suggestion_by_label<'completion>(
    completion_suggestions: &'completion [CompletionSuggestion],
    label: &str,
) -> &'completion CompletionSuggestion {
    completion_suggestions
        .iter()
        .find(|completion_suggestion| completion_suggestion.label == label)
        .unwrap_or_else(|| panic!("expected completion label `{label}` to exist"))
}

fn diagnostics_from_template(source_template: &str) -> Vec<DocumentDiagnostic> {
    let source_template = normalize_rust_doc_comment_tokens(source_template);
    let source = normalize_inline_cursor_layout(&source_template);
    let document_state = DocumentState::new(source, None);

    document_state.diagnostics()
}

fn normalize_rust_doc_comment_tokens(source_template: &str) -> String {
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

fn normalize_inline_cursor_layout(source_template: &str) -> String {
    let compact_marker = "<cursor>";
    let spaced_marker = "< cursor >";

    let compact_marker_offset = source_template.find(compact_marker);
    let spaced_marker_offset = source_template.find(spaced_marker);

    let (marker, marker_offset) = match (compact_marker_offset, spaced_marker_offset) {
        (Some(compact_offset), Some(spaced_offset)) => {
            if compact_offset <= spaced_offset {
                (compact_marker, compact_offset)
            } else {
                (spaced_marker, spaced_offset)
            }
        }
        (Some(compact_offset), None) => (compact_marker, compact_offset),
        (None, Some(spaced_offset)) => (spaced_marker, spaced_offset),
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

mod completion_tests;
mod definition_tests;
mod diagnostic_tests;
mod for_loop_tests;
mod interpolation_tests;
mod tool_tests;
