use super::{CompletionSuggestion, DocumentDiagnostic, DocumentState, Position, TypeExpression};
use crate::diagnostic_code::DiagnosticCode;
use lsp_types::CompletionItemKind;
use superwire_core::dsl::{
    AgentExpressionPropertyName, BuiltinFunctionName, DeclarationKeyword, ForClauseKeyword, McpCallOperation, ReferenceKeyword,
    SingletonDeclarationKind, ToolCallKeyword,
};
use superwire_core::semantic::InferenceSetting;
use superwire_core::testing::WorkflowSourceTemplate;

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
    let source_with_cursor = WorkflowSourceTemplate::from_inline(source_template).with_cursor();
    let cursor_position = source_with_cursor.cursor_position();

    (
        source_with_cursor.into_source(),
        Position {
            line: cursor_position.line,
            character: cursor_position.character,
        },
    )
}

fn source_without_cursor_normalization(source_template: &str) -> (String, Position) {
    let source_with_cursor = WorkflowSourceTemplate::from_inline(source_template).without_cursor_normalization();
    let cursor_position = source_with_cursor.cursor_position();

    (
        source_with_cursor.into_source(),
        Position {
            line: cursor_position.line,
            character: cursor_position.character,
        },
    )
}

fn completion_suggestions_from_template(source_template: &str) -> Vec<CompletionSuggestion> {
    let (source, cursor_position) = source_with_cursor(source_template);

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
    let source = WorkflowSourceTemplate::from_inline(source_template)
        .normalized_cursor_layout()
        .source()
        .to_string();
    let document_state = DocumentState::new(source, None);

    document_state.diagnostics()
}

#[test]
fn replace_text_skips_semantic_snapshot_rebuild_when_document_is_unchanged() {
    let (source, _cursor_position) = source_with_cursor(inline_document_template! {
        output {
            value: <cursor>"ok"
        }
    });
    let mut document_state = DocumentState::new(source.clone(), None);

    assert!(!document_state.replace_text(source, None));
}

mod completion_tests;
mod definition_tests;
mod diagnostic_tests;
mod for_loop_tests;
mod interpolation_tests;
mod tool_tests;
