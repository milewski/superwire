use super::{CompletionSuggestion, DocumentDiagnostic, DocumentState, Position, TypeExpression};
use crate::diagnostic_code::DiagnosticCode;
use lsp_types::CompletionItemKind;
use std::collections::BTreeMap;
use superwire_dsl::{
    AgentExpressionPropertyName, BuiltinFunctionName, DeclarationKeyword, ExpressionKeyword, ForClauseKeyword, McpCallOperation,
    ReferenceKeyword, SingletonDeclarationKind, ToolCallKeyword,
};
use superwire_mcp::{McpLock, McpPromptArgumentLock, McpServerLock, McpToolLock};
use superwire_semantic::InferenceSetting;
use superwire_test_support::WorkflowSourceTemplate;

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
        vec![Self::Template.as_str()]
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

impl CompletionLabel for ExpressionKeyword {
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

fn completion_suggestions_with_mcp_lock(source_template: &str) -> Vec<CompletionSuggestion> {
    let (source, cursor_position) = source_with_cursor(source_template);
    let document_state = DocumentState::new(source, Some(test_mcp_lock()));

    document_state.completion_suggestions(cursor_position)
}

fn completion_suggestions_with_mcp_lock_without_cursor_normalization(source_template: &str) -> Vec<CompletionSuggestion> {
    let compact_cursor_marker = "<cursor>";
    let spaced_cursor_marker = "< cursor >";
    let (cursor_marker, cursor_byte_offset) = if let Some(cursor_byte_offset) = source_template.find(compact_cursor_marker) {
        (compact_cursor_marker, cursor_byte_offset)
    } else if let Some(cursor_byte_offset) = source_template.find(spaced_cursor_marker) {
        (spaced_cursor_marker, cursor_byte_offset)
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
    let document_state = DocumentState::new(source_without_cursor, Some(test_mcp_lock()));

    document_state.completion_suggestions(Position { line, character })
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

fn test_mcp_lock() -> McpLock {
    McpLock {
        servers: test_mcp_servers(test_mcp_tools()),
    }
}

fn test_mcp_servers(tools: BTreeMap<String, McpToolLock>) -> BTreeMap<String, McpServerLock> {
    BTreeMap::from([(
        "local".to_string(),
        McpServerLock {
            tools,
            resources: vec!["project-readme".to_string(), "release-notes".to_string()],
            prompts: vec![
                "system-prompt".to_string(),
                "review-prompt".to_string(),
                "dynamic-summary-prompt".to_string(),
            ],
            prompt_arguments: BTreeMap::from([(
                "dynamic-summary-prompt".to_string(),
                vec![
                    McpPromptArgumentLock {
                        name: "project_id".to_string(),
                        required: true,
                        description: Some("Project identifier to summarize".to_string()),
                    },
                    McpPromptArgumentLock {
                        name: "user_id".to_string(),
                        required: false,
                        description: Some("Optional user context for personalization".to_string()),
                    },
                ],
            )]),
        },
    )])
}

fn test_mcp_tools() -> BTreeMap<String, McpToolLock> {
    BTreeMap::from([
        (
            "list_all_participants_who_has_answered_given_task".to_string(),
            list_all_participants_tool_lock(),
        ),
        ("update-user-name".to_string(), update_user_name_tool_lock()),
        ("get_task_group_tasks".to_string(), get_task_group_tasks_tool_lock()),
        ("fetch_participant_answer".to_string(), fetch_participant_answer_tool_lock()),
    ])
}

fn list_all_participants_tool_lock() -> McpToolLock {
    McpToolLock::from_json_schema_values(
        "list_all_participants_who_has_answered_given_task".to_string(),
        Some("List all participants who answered a task".to_string()),
        serde_json::json!({
            "type": "object",
            "properties": {
                "common_shared_among_all_feature": { "type": "string" },
                "project_id": { "description": "Project identifier", "type": "number" },
                "task_id": { "type": "number" }
            },
            "required": ["common_shared_among_all_feature", "project_id", "task_id"]
        }),
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "shared": { "type": "string" },
                "participants": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["shared", "participants"]
        })),
    )
    .expect("test MCP input schema should parse")
}

fn update_user_name_tool_lock() -> McpToolLock {
    McpToolLock::from_json_schema_values(
        "update-user-name".to_string(),
        Some("Update a user name".to_string()),
        serde_json::json!({
            "type": "object",
            "properties": {
                "common_shared_among_all_feature": { "type": "string" },
                "user_name": { "type": "string" }
            },
            "required": ["common_shared_among_all_feature", "user_name"]
        }),
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "shared": { "type": "string" },
                "success": { "type": "boolean" }
            },
            "required": ["shared", "success"]
        })),
    )
    .expect("test MCP input schema should parse")
}

fn get_task_group_tasks_tool_lock() -> McpToolLock {
    McpToolLock::from_json_schema_values(
        "get_task_group_tasks".to_string(),
        Some("Get task group tasks".to_string()),
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "task_group_id": { "type": "number" },
                "task_group_title": { "type": "string" },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "duration": { "type": "number" },
                            "id": { "type": "number" },
                            "mandatory": { "type": "boolean" },
                            "options": { "type": "string" },
                            "title": { "type": "string" },
                            "type": { "type": "string" }
                        },
                        "required": ["description", "duration", "id", "mandatory", "options", "title", "type"]
                    }
                }
            },
            "required": ["task_group_id", "task_group_title", "tasks"]
        })),
    )
    .expect("test MCP task group schema should parse")
}

fn fetch_participant_answer_tool_lock() -> McpToolLock {
    McpToolLock::from_json_schema_values(
        "fetch_participant_answer".to_string(),
        Some("Fetch participant answer".to_string()),
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "answer": {
                    "description": "Answer",
                    "type": "object",
                    "properties": {
                        "text": {
                            "description": "The text content of the answer",
                            "type": ["string", "null"]
                        }
                    },
                    "required": ["text"]
                },
                "participant_id": {
                    "description": "The ID of the participant",
                    "type": "number"
                },
                "task_id": {
                    "description": "The ID of the task",
                    "type": "number"
                }
            },
            "required": ["answer", "participant_id", "task_id"]
        })),
    )
    .expect("test MCP nullable schema should parse")
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

mod completion_asset_tests;
mod completion_matrix_tests;
mod completion_mcp_tests;
mod completion_model_tests;
mod completion_reference_tests;
mod completion_root_tests;
mod completion_schema_tests;
mod completion_tool_tests;
mod definition_tests;
mod diagnostic_agent_tests;
mod diagnostic_mcp_tests;
mod diagnostic_model_tests;
mod diagnostic_reference_tests;
mod diagnostic_schema_tests;
mod diagnostic_syntax_tests;
mod diagnostic_tool_tests;
mod editing_workflow_tests;
mod for_loop_tests;
mod interpolation_tests;
