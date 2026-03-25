use std::collections::HashSet;

mod completion;
mod completion_context;
mod reference;
mod scope;
mod semantic_index;
mod snapshot;
mod types;

use snapshot::SemanticSnapshot;
pub use types::{CompletionKind, CompletionSuggestion, DiagnosticSeverity, DocumentDiagnostic};

use engine_ai_core::dsl::{DeclarationKeyword, SingletonDeclarationKind, SourcePosition, SourceSpan, TypeExpression};
use engine_ai_core::runtime::ProviderDriver;

use crate::protocol::{Position, Range};

#[derive(Debug)]
pub struct DocumentState {
    text: String,
    semantic_snapshot: SemanticSnapshot,
}

impl DocumentState {
    #[must_use]
    pub fn new(text: String) -> Self {
        let semantic_snapshot = SemanticSnapshot::from_text(&text);

        Self { text, semantic_snapshot }
    }

    pub fn replace_text(&mut self, text: String) {
        self.semantic_snapshot = SemanticSnapshot::from_text(&text);
        self.text = text;
    }

    #[must_use]
    pub fn diagnostics(&self) -> Vec<DocumentDiagnostic> {
        self.semantic_snapshot.diagnostics(&self.text)
    }

    #[must_use]
    pub fn hover_markdown(&self, position: Position) -> Option<String> {
        let hovered_symbol = self.symbol_at(position)?;

        if let Some(symbol_markdown) = builtin_symbol_markdown(&hovered_symbol) {
            return Some(symbol_markdown);
        }

        self.semantic_snapshot.semantic_index.hover_markdown(&hovered_symbol)
    }

    fn line_prefix(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let cursor_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().take(cursor_index).collect())
    }

    fn symbol_at(&self, position: Position) -> Option<String> {
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
}

fn all_provider_property_names() -> Vec<&'static str> {
    let mut property_name_set = HashSet::<&'static str>::new();

    for provider_driver in ProviderDriver::all() {
        for property_name in provider_driver.available_property_names() {
            property_name_set.insert(*property_name);
        }
    }

    let mut property_names = property_name_set.into_iter().collect::<Vec<_>>();
    property_names.sort_unstable();

    property_names
}

fn is_symbol_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?'
}

fn source_span_to_range(source_text: &str, source_span: SourceSpan) -> Range {
    let start = source_position_to_position(source_span.start);
    let mut end = source_position_to_position(source_span.end);

    if end.line < start.line || (end.line == start.line && end.character <= start.character) {
        end = Position {
            line: start.line,
            character: start.character.saturating_add(1),
        };

        if let Some(line_length) = line_character_count(source_text, start.line) {
            end.character = end.character.min(u32_from_usize_saturating(line_length));
        }
    }

    Range { start, end }
}

fn line_character_count(source_text: &str, line_index: u32) -> Option<usize> {
    source_text
        .lines()
        .nth(line_index as usize)
        .map(|line_text| line_text.chars().count())
}

fn source_position_to_position(source_position: SourcePosition) -> Position {
    Position {
        line: u32_from_usize_saturating(source_position.line.saturating_sub(1)),
        character: u32_from_usize_saturating(source_position.column.saturating_sub(1)),
    }
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn source_span_contains_position(source_span: SourceSpan, position: Position) -> bool {
    let target_line = position.line as usize + 1;
    let target_column = position.character as usize + 1;

    let starts_before_or_at =
        (source_span.start.line < target_line) || (source_span.start.line == target_line && source_span.start.column <= target_column);

    let ends_after_or_at =
        (source_span.end.line > target_line) || (source_span.end.line == target_line && source_span.end.column >= target_column);

    starts_before_or_at && ends_after_or_at
}

fn zero_range() -> Range {
    Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 1 },
    }
}

trait RenderTypeExpression {
    fn render_type(&self) -> String;
}

impl RenderTypeExpression for TypeExpression {
    fn render_type(&self) -> String {
        match self {
            Self::String => "string".to_string(),
            Self::Number => "number".to_string(),
            Self::Float => "float".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::Null => "null".to_string(),
            Self::SchemaReference(schema_name) => format!("schema.{schema_name}"),
            Self::StringEnum(enum_value) => format!("\"{enum_value}\""),
            Self::Array { item_type, fixed_length } => {
                if let Some(fixed_length) = fixed_length {
                    return format!("[{}; {fixed_length}]", item_type.render_type());
                }

                format!("[{}]", item_type.render_type())
            }
            Self::Tuple(tuple_items) => {
                let tuple_item_strings = tuple_items
                    .iter()
                    .map(RenderTypeExpression::render_type)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("({tuple_item_strings})")
            }
            Self::Object(typed_fields) => {
                let field_strings = typed_fields
                    .iter()
                    .map(|typed_field| format!("{}: {}", typed_field.name, typed_field.field_type.render_type()))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{{ {field_strings} }}")
            }
            Self::Union(union_members) => union_members
                .iter()
                .map(RenderTypeExpression::render_type)
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BuiltinSymbolDoc {
    label: &'static str,
    kind: CompletionKind,
    detail: &'static str,
    documentation: &'static str,
}

const BUILTIN_SYMBOL_DOCS: [BuiltinSymbolDoc; 8] = [
    BuiltinSymbolDoc {
        label: "tool",
        kind: CompletionKind::Module,
        detail: "Tool namespace",
        documentation: "Use `tool.<name>` to reference declared tools.",
    },
    BuiltinSymbolDoc {
        label: "context",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Returns serialized context for `agent.<name>`.",
    },
    BuiltinSymbolDoc {
        label: "template",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Renders a string template from source and bindings.",
    },
    BuiltinSymbolDoc {
        label: "compact",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Compacts nullable values in object-like data.",
    },
    BuiltinSymbolDoc {
        label: "string",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "String type.",
    },
    BuiltinSymbolDoc {
        label: "number",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Integer number type.",
    },
    BuiltinSymbolDoc {
        label: "float",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Floating-point number type.",
    },
    BuiltinSymbolDoc {
        label: "boolean",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Boolean type.",
    },
];

trait DeclarationKeywordCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl DeclarationKeywordCompletionDoc for DeclarationKeyword {
    fn completion_detail(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Provider declaration",
            DeclarationKeyword::Secrets => "Secrets declaration",
            DeclarationKeyword::Input => "Input declaration",
            DeclarationKeyword::Schema => "Schema declaration",
            DeclarationKeyword::Agent => "Agent declaration",
            DeclarationKeyword::Output => "Output declaration",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Declares a provider configuration block.",
            DeclarationKeyword::Secrets => "Declares workflow secret fields.",
            DeclarationKeyword::Input => "Declares workflow input fields.",
            DeclarationKeyword::Schema => "Declares a reusable named schema type.",
            DeclarationKeyword::Agent => "Declares an executable workflow agent.",
            DeclarationKeyword::Output => "Declares final workflow output fields.",
        }
    }
}

fn builtin_symbol_suggestions(include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
    builtin_symbol_docs()
        .filter(|builtin_symbol_doc| include_builtin_function_suggestions || !matches!(builtin_symbol_doc.kind, CompletionKind::Function))
        .map(|builtin_symbol_doc| CompletionSuggestion {
            label: builtin_symbol_doc.label.to_string(),
            kind: builtin_symbol_doc.kind,
            detail: builtin_symbol_doc.detail.to_string(),
            documentation: builtin_symbol_doc.documentation.to_string(),
            insert_text: builtin_symbol_doc.label.to_string(),
        })
        .collect()
}

fn type_symbol_suggestions() -> Vec<CompletionSuggestion> {
    primitive_type_expressions()
        .into_iter()
        .map(|primitive_type_expression| {
            let type_name = primitive_type_expression.render_type();

            CompletionSuggestion {
                label: type_name.clone(),
                kind: CompletionKind::Type,
                detail: "Primitive type".to_string(),
                documentation: "Primitive workflow type.".to_string(),
                insert_text: type_name.clone(),
            }
        })
        .collect()
}

fn builtin_symbol_markdown(symbol_name: &str) -> Option<String> {
    let direct_match = find_builtin_symbol_doc(symbol_name).or_else(|| symbol_name.rsplit('.').next().and_then(find_builtin_symbol_doc))?;

    Some(format!(
        "**{}**\n\n{}\n\n{}",
        direct_match.label, direct_match.detail, direct_match.documentation
    ))
}

fn primitive_type_expressions() -> [TypeExpression; 5] {
    [
        TypeExpression::String,
        TypeExpression::Number,
        TypeExpression::Float,
        TypeExpression::Boolean,
        TypeExpression::Null,
    ]
}

fn declaration_builtin_symbol_docs() -> [BuiltinSymbolDoc; 6] {
    [
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Provider.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Provider.completion_detail(),
            documentation: DeclarationKeyword::Provider.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Agent.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Agent.completion_detail(),
            documentation: DeclarationKeyword::Agent.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Schema.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Schema.completion_detail(),
            documentation: DeclarationKeyword::Schema.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Input.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Input.completion_detail(),
            documentation: DeclarationKeyword::Input.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Secrets.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Secrets.completion_detail(),
            documentation: DeclarationKeyword::Secrets.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Output.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Output.completion_detail(),
            documentation: DeclarationKeyword::Output.completion_documentation(),
        },
    ]
}

fn builtin_symbol_docs() -> impl Iterator<Item = BuiltinSymbolDoc> {
    declaration_builtin_symbol_docs().into_iter().chain(BUILTIN_SYMBOL_DOCS)
}

fn find_builtin_symbol_doc(symbol_name: &str) -> Option<BuiltinSymbolDoc> {
    builtin_symbol_docs().find(|builtin_symbol_doc| builtin_symbol_doc.label == symbol_name)
}

#[cfg(test)]
mod tests {
    use super::{CompletionKind, CompletionSuggestion, DocumentState, Position, TypeExpression};
    use crate::protocol::DiagnosticCode;
    use engine_ai_core::dsl::{
        AgentExpressionPropertyName, BuiltinFunctionName, DeclarationKeyword, ReferenceKeyword, SingletonDeclarationKind,
    };
    use engine_ai_core::runtime::InferenceSetting;

    macro_rules! inline_document_with_cursor {
        ($($workflow_tokens:tt)*) => {{
            source_with_cursor(stringify!($($workflow_tokens)*))
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
                    .map(|completion_suggestion| {
                        (
                            completion_suggestion.label.clone(),
                            std::mem::discriminant(&completion_suggestion.kind),
                        )
                    })
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
                "unexpected completion label `{label_in_group}` from group; available labels: {:?}",
                available_labels
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
                "expected completion label `{label_in_group}` from group; available labels: {:?}",
                available_labels
            );
        }
    }

    fn diagnostic_has_code(diagnostics: &[super::DocumentDiagnostic], expected_code: DiagnosticCode) -> bool {
        diagnostics.iter().any(|diagnostic| diagnostic.code == expected_code)
    }

    fn expected_completion_label<Label>(label_value: Label) -> &'static str
    where
        Label: CompletionLabel,
    {
        label_value.as_completion_label()
    }

    trait CompletionLabel {
        fn as_completion_label(self) -> &'static str;
    }

    trait CompletionLabelGroup {
        fn completion_labels() -> Vec<&'static str>;
    }

    impl CompletionLabel for &'static str {
        fn as_completion_label(self) -> &'static str {
            self
        }
    }

    impl CompletionLabel for InferenceSetting {
        fn as_completion_label(self) -> &'static str {
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
        fn as_completion_label(self) -> &'static str {
            self.as_str()
        }
    }

    impl CompletionLabel for BuiltinFunctionName {
        fn as_completion_label(self) -> &'static str {
            self.as_str()
        }
    }

    impl CompletionLabel for ReferenceKeyword {
        fn as_completion_label(self) -> &'static str {
            self.as_str()
        }
    }

    impl CompletionLabel for SingletonDeclarationKind {
        fn as_completion_label(self) -> &'static str {
            self.as_str()
        }
    }

    impl CompletionLabel for DeclarationKeyword {
        fn as_completion_label(self) -> &'static str {
            self.as_str()
        }
    }

    impl CompletionLabel for TypeExpression {
        fn as_completion_label(self) -> &'static str {
            match self {
                TypeExpression::String => "string",
                TypeExpression::Number => "number",
                TypeExpression::Float => "float",
                TypeExpression::Boolean => "boolean",
                TypeExpression::Null => "null",
                TypeExpression::SchemaReference(_)
                | TypeExpression::StringEnum(_)
                | TypeExpression::Array {
                    item_type: _,
                    fixed_length: _,
                }
                | TypeExpression::Tuple(_)
                | TypeExpression::Object(_)
                | TypeExpression::Union(_) => {
                    panic!("completion label is only defined for primitive TypeExpression variants")
                }
            }
        }
    }

    fn source_with_cursor(source_template: &str) -> (String, Position) {
        let normalized_template = normalize_inline_cursor_layout(source_template);
        let compact_cursor_marker = "<cursor>";

        let (cursor_marker, cursor_byte_offset) = if let Some(marker_offset) = normalized_template.find(compact_cursor_marker) {
            (compact_cursor_marker, marker_offset)
        } else {
            panic!("cursor marker should exist in test source");
        };

        let mut line = 0_u32;
        let mut character = 0_u32;

        for character_in_source in normalized_template[..cursor_byte_offset].chars() {
            if character_in_source == '\n' {
                line += 1;
                character = 0;
                continue;
            }

            character += 1;
        }

        let source_without_cursor = normalized_template.replacen(cursor_marker, "", 1);

        (source_without_cursor, Position { line, character })
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

        if previous_character == Some('.') || previous_character == Some(':') {
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CompletionMatrixContext {
        Declarations,
        AgentProperties,
        InferenceBlock,
        TypedDeclarations,
        Interpolation,
        ForLoopIterable,
        Tools,
    }

    impl CompletionMatrixContext {
        fn all() -> [Self; 7] {
            [
                Self::Declarations,
                Self::AgentProperties,
                Self::InferenceBlock,
                Self::TypedDeclarations,
                Self::Interpolation,
                Self::ForLoopIterable,
                Self::Tools,
            ]
        }

        fn display_name(self) -> &'static str {
            match self {
                Self::Declarations => "declarations",
                Self::AgentProperties => "agent_properties",
                Self::InferenceBlock => "inference_block",
                Self::TypedDeclarations => "typed_declarations",
                Self::Interpolation => "interpolation",
                Self::ForLoopIterable => "for_loop_iterable",
                Self::Tools => "tools",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CompletionExpectationKind {
        Positive,
        Negative,
    }

    impl CompletionExpectationKind {
        fn display_name(self) -> &'static str {
            match self {
                Self::Positive => "positive",
                Self::Negative => "negative",
            }
        }
    }

    struct CompletionMatrixCase {
        case_name: &'static str,
        context: CompletionMatrixContext,
        expectation_kind: CompletionExpectationKind,
        source_template: &'static str,
        expected_present_labels: Vec<&'static str>,
        expected_absent_labels: Vec<&'static str>,
        expects_empty_suggestions: bool,
    }

    fn completion_matrix_cases() -> Vec<CompletionMatrixCase> {
        vec![
            CompletionMatrixCase {
                case_name: "top_level_declares_keywords",
                context: CompletionMatrixContext::Declarations,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                <cursor>

                output {
                    value: null
                }
                "#,
                expected_present_labels: vec![DeclarationKeyword::Provider.as_str(), DeclarationKeyword::Agent.as_str()],
                expected_absent_labels: vec![BuiltinFunctionName::Context.as_str()],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "agent_block_excludes_declaration_keywords",
                context: CompletionMatrixContext::Declarations,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                agent writer {
                    <cursor>
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec![DeclarationKeyword::Provider.as_str(), DeclarationKeyword::Schema.as_str()],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "agent_block_suggests_agent_properties",
                context: CompletionMatrixContext::AgentProperties,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                agent writer {
                    <cursor>
                }
                "#,
                expected_present_labels: vec![
                    AgentExpressionPropertyName::Model.as_str(),
                    AgentExpressionPropertyName::Prompt.as_str(),
                ],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "inference_object_excludes_agent_properties",
                context: CompletionMatrixContext::AgentProperties,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                agent writer {
                    inference: {
                        <cursor>
                    }
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec![
                    AgentExpressionPropertyName::Model.as_str(),
                    AgentExpressionPropertyName::Prompt.as_str(),
                ],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "inference_object_suggests_inference_settings",
                context: CompletionMatrixContext::InferenceBlock,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                agent writer {
                    inference: {
                        <cursor>
                    }
                }
                "#,
                expected_present_labels: vec![InferenceSetting::Temperature.key(), InferenceSetting::MaxTokens.key()],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "agent_scope_excludes_inference_settings",
                context: CompletionMatrixContext::InferenceBlock,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                agent release_analyst {
                    model: openai("gpt-4.1-mini")

                    <cursor>

                    inference: {
                        temperature: 0.2
                        max_tokens: 12_000
                    }
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec![InferenceSetting::Temperature.key(), InferenceSetting::MaxTokens.key()],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "typed_declaration_suggests_primitive_types",
                context: CompletionMatrixContext::TypedDeclarations,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                input {
                    product_name: <cursor>
                }
                "#,
                expected_present_labels: vec![
                    TypeExpression::String.as_completion_label(),
                    TypeExpression::Number.as_completion_label(),
                ],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "input_key_position_excludes_typed_declaration_values",
                context: CompletionMatrixContext::TypedDeclarations,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                input {
                    <cursor>
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec![
                    TypeExpression::String.as_completion_label(),
                    TypeExpression::Number.as_completion_label(),
                ],
                expects_empty_suggestions: true,
            },
            CompletionMatrixCase {
                case_name: "interpolation_suggests_agent_references",
                context: CompletionMatrixContext::Interpolation,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                provider openai {
                    driver: "openai"
                    models: ["gpt-4.1-mini"]
                }

                agent context_agent {
                    model: openai("gpt-4.1-mini")
                    prompt: "hello"
                    output: string
                }

                agent worker {
                    model: openai("gpt-4.1-mini")
                    prompt: "example {{ agent.<cursor> }}"
                    output: string
                }
                "#,
                expected_present_labels: vec!["context_agent"],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "interpolation_excludes_current_agent_reference",
                context: CompletionMatrixContext::Interpolation,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                provider openai {
                    driver: "openai"
                    models: ["gpt-4.1-mini"]
                }

                agent context_agent {
                    model: openai("gpt-4.1-mini")
                    prompt: "hello"
                    output: string
                }

                agent worker {
                    model: openai("gpt-4.1-mini")
                    prompt: "example {{ agent.<cursor> }}"
                    output: string
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec!["worker"],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "for_loop_iterable_suggests_iterable_fields",
                context: CompletionMatrixContext::ForLoopIterable,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                input {
                    products: [string]
                }

                agent worker for item in input.<cursor> {
                    prompt: item
                }
                "#,
                expected_present_labels: vec!["products"],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "for_loop_iterable_excludes_non_iterable_fields",
                context: CompletionMatrixContext::ForLoopIterable,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                input {
                    product_name: string
                }

                agent worker for item in input.<cursor> {
                    prompt: item
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec!["product_name"],
                expects_empty_suggestions: true,
            },
            CompletionMatrixCase {
                case_name: "tools_expression_suggests_tool_keyword",
                context: CompletionMatrixContext::Tools,
                expectation_kind: CompletionExpectationKind::Positive,
                source_template: r#"
                agent tooling {
                    tools: <cursor>
                }
                "#,
                expected_present_labels: vec![ReferenceKeyword::Tool.as_str()],
                expected_absent_labels: vec![],
                expects_empty_suggestions: false,
            },
            CompletionMatrixCase {
                case_name: "tool_namespace_excludes_member_suggestions",
                context: CompletionMatrixContext::Tools,
                expectation_kind: CompletionExpectationKind::Negative,
                source_template: r#"
                agent tooling {
                    tools: [tool.<cursor>]
                }
                "#,
                expected_present_labels: vec![],
                expected_absent_labels: vec![],
                expects_empty_suggestions: true,
            },
        ]
    }

    #[test]
    fn completion_behavior_matrix_covers_primary_contexts() {
        let completion_matrix_cases = completion_matrix_cases();

        for completion_matrix_context in CompletionMatrixContext::all() {
            assert!(
                completion_matrix_cases.iter().any(|completion_matrix_case| {
                    completion_matrix_case.context == completion_matrix_context
                        && completion_matrix_case.expectation_kind == CompletionExpectationKind::Positive
                }),
                "completion matrix should include a positive case for context {}",
                completion_matrix_context.display_name()
            );

            assert!(
                completion_matrix_cases.iter().any(|completion_matrix_case| {
                    completion_matrix_case.context == completion_matrix_context
                        && completion_matrix_case.expectation_kind == CompletionExpectationKind::Negative
                }),
                "completion matrix should include a negative case for context {}",
                completion_matrix_context.display_name()
            );
        }

        for completion_matrix_case in completion_matrix_cases {
            let (source, cursor_position) = source_with_cursor(completion_matrix_case.source_template);
            let document_state = DocumentState::new(source);
            let completion_suggestions = document_state.completion_suggestions(cursor_position);
            let available_labels = completion_label_set(&completion_suggestions);
            let mut sorted_available_labels = available_labels.into_iter().collect::<Vec<_>>();

            sorted_available_labels.sort_unstable();

            if completion_matrix_case.expects_empty_suggestions {
                assert!(
                    completion_suggestions.is_empty(),
                    "case `{}` ({}/{}) expected empty completion suggestions; got labels {:?}",
                    completion_matrix_case.case_name,
                    completion_matrix_case.context.display_name(),
                    completion_matrix_case.expectation_kind.display_name(),
                    sorted_available_labels
                );

                continue;
            }

            for expected_label in completion_matrix_case.expected_present_labels {
                assert!(
                    sorted_available_labels.contains(&expected_label),
                    "case `{}` ({}/{}) expected label `{}`; available labels {:?}",
                    completion_matrix_case.case_name,
                    completion_matrix_case.context.display_name(),
                    completion_matrix_case.expectation_kind.display_name(),
                    expected_label,
                    sorted_available_labels
                );
            }

            for unexpected_label in completion_matrix_case.expected_absent_labels {
                assert!(
                    !sorted_available_labels.contains(&unexpected_label),
                    "case `{}` ({}/{}) should not include label `{}`; available labels {:?}",
                    completion_matrix_case.case_name,
                    completion_matrix_case.context.display_name(),
                    completion_matrix_case.expectation_kind.display_name(),
                    unexpected_label,
                    sorted_available_labels
                );
            }
        }
    }

    #[test]
    fn reports_parse_diagnostics_for_invalid_syntax() {
        let document_state = DocumentState::new("agent broken {\n    prompt: \"hello\"\n".to_string());
        let diagnostics = document_state.diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ParseError);
    }

    #[test]
    fn reports_unknown_model_for_provider_diagnostic() {
        let (source, _cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent writer {
                model: openai("gpt-4.1")
                prompt: "hello"
                output: string
            }
            <cursor>
        };

        let document_state = DocumentState::new(source);
        let diagnostics = document_state.diagnostics();

        assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownModelForProvider);
    }

    #[test]
    fn reports_unknown_agent_property_diagnostic() {
        let (source, _cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent writer {
                model: openai("gpt-4.1-mini")
                prompt: "hello"
                retries: 3
                output: string
            }
            <cursor>
        };

        let document_state = DocumentState::new(source);
        let diagnostics = document_state.diagnostics();

        assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnknownAgentProperty);
    }

    #[test]
    fn reports_invalid_bare_tool_reference_diagnostic() {
        let (source, _cursor_position) = inline_document_with_cursor! {
            agent tooling {
                tools: [tool]
            }

            <cursor>
        };

        let document_state = DocumentState::new(source);
        let diagnostics = document_state.diagnostics();

        assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidKeywordReferenceRoot);
    }

    #[test]
    fn completes_nested_input_field_attributes() {
        let (source, cursor_position) = inline_document_with_cursor! {
            input {
                profile: {
                    name: {
                        first: string
                        last: string
                    }
                }
            }

            output {
                value: input.profile.name.<cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "first", "last");
    }

    #[test]
    fn completes_input_fields_in_for_loop_iterable_reference() {
        let (source, cursor_position) = inline_document_with_cursor! {
            input {
                products: [string]
            }

            agent worker for item in input.<cursor> {
                prompt: item
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "products");
    }

    #[test]
    fn completes_agent_references_inside_prompt_string_interpolation() {
        let (source, cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent context_agent {
                model: openai("gpt-4.1-mini")
                prompt: "hello"
                output: string
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: "example {{ agent.<cursor> }}"
                output: string
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "context_agent");
        assert_completion_excludes_labels!(&completion_suggestions, "worker");
    }

    #[test]
    fn completes_agent_references_inside_multiline_prompt_string_interpolation() {
        let (source, cursor_position) = source_with_cursor(
            r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent context_agent {
                model: openai("gpt-4.1-mini")
                prompt: "hello"
                output: string
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: """
                    example {{ agent.<cursor> }}
                """
                output: string
            }
            "#,
        );

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "context_agent");
    }

    #[test]
    fn suppresses_suggestions_inside_plain_multiline_prompt_string_text() {
        let (source, cursor_position) = source_with_cursor(
            r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: """
                    Like this <cursor>
                """
                output: string
            }
            "#,
        );

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert!(completion_suggestions.is_empty());
    }

    #[test]
    fn reports_secret_reference_in_prompt_string_interpolation_diagnostic() {
        let (source, _cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            schema Payload {
                value: string
            }

            input {
                query: string
            }

            secrets {
                api_key: string
            }

            agent context_agent {
                model: openai("gpt-4.1-mini")
                prompt: "hello"
                output: string
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: "example {{ agent.context_agent }} {{ input.query }} {{ schema.Payload }} {{ secrets.api_key }}"
                output: string
            }

            <cursor>
        };

        let document_state = DocumentState::new(source);
        let diagnostics = document_state.diagnostics();

        assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
    }

    #[test]
    fn reports_secret_reference_in_multiline_prompt_string_interpolation_diagnostic() {
        let source = r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            input {
                query: string
            }

            secrets {
                api_key: string
            }

            agent worker {
                model: openai("gpt-4.1-mini")
                prompt: """
                    example {{ input.query }}
                    forbidden {{ secrets.api_key }}
                """
                output: string
            }
        "#;

        let document_state = DocumentState::new(source.to_string());
        let diagnostics = document_state.diagnostics();

        assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::SecretReferenceInLlmContext);
    }

    #[test]
    fn suppresses_non_iterable_input_field_suggestions_in_for_loop_iterable_reference() {
        let (source, cursor_position) = inline_document_with_cursor! {
            input {
                xxxx: string
            }

            agent worker for item in input.<cursor> {
                prompt: item
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert!(completion_suggestions.is_empty());
    }

    #[test]
    fn suggests_tool_keyword_inside_tools_expression_context() {
        let (source, cursor_position) = source_with_cursor(
            r#"
            agent tooling {
                tools: <cursor>
            }
            "#,
        );

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Tool);
    }

    #[test]
    fn suppresses_member_suggestions_for_tool_namespace_reference() {
        let (source, cursor_position) = inline_document_with_cursor! {
            agent tooling {
                tools: [tool.<cursor>]
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert!(completion_suggestions.is_empty());
    }

    #[test]
    fn suggests_agent_properties_inside_for_loop_agent_block() {
        let (source, cursor_position) = inline_document_with_cursor! {
            agent source {}

            agent worker for item in agent.source {
                <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, AgentExpressionPropertyName::Prompt);
        assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
    }

    #[test]
    fn completes_provider_driver_specific_properties() {
        let (source, cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "endpoint", "api_key");
    }

    #[test]
    fn suppresses_builtin_functions_in_top_level_scope() {
        let (source, cursor_position) = inline_document_with_cursor! {
            <cursor>

            output {
                value: null
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_label_groups!(&completion_suggestions, SingletonDeclarationKind);

        assert_completion_contains_labels!(&completion_suggestions, ReferenceKeyword::Agent, ReferenceKeyword::Tool);
        assert_completion_excludes_labels!(&completion_suggestions, BuiltinFunctionName);
    }

    #[test]
    fn suggests_builtin_functions_in_output_expression_context() {
        let (source, cursor_position) = inline_document_with_cursor! {
            output {
                value: <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_label_groups!(&completion_suggestions, BuiltinFunctionName);
    }

    #[test]
    fn suggests_only_agent_properties_in_agent_block_scope() {
        let (source, cursor_position) = inline_document_with_cursor! {
            agent writer {
                <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_labels!(
            &completion_suggestions,
            AgentExpressionPropertyName::Model,
            AgentExpressionPropertyName::Prompt,
            "output"
        );

        assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider);
        assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Function);
    }

    #[test]
    fn suggests_only_inference_settings_inside_inference_object() {
        let (source, cursor_position) = inline_document_with_cursor! {
            agent writer {
                inference: {
                    <cursor>
                }
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_all_inference_settings!(&completion_suggestions);

        assert_completion_excludes_labels!(
            &completion_suggestions,
            AgentExpressionPropertyName::Model,
            DeclarationKeyword::Provider
        );

        assert_completion_excludes_kind!(&completion_suggestions, CompletionKind::Function);
    }

    #[test]
    fn suggests_agent_properties_before_inference_block() {
        let (source, cursor_position) = inline_document_with_cursor! {
            agent release_analyst {
                model: openai("gpt-4.1-mini")

                <cursor>

                inference: {
                    temperature: 0.2
                    max_tokens: 12_000
                }
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_labels!(&completion_suggestions, AgentExpressionPropertyName::Prompt);
        assert_completion_excludes_labels!(&completion_suggestions, InferenceSetting);
    }

    #[test]
    fn includes_descriptive_details_for_agent_and_inference_completions() {
        let (agent_source, agent_cursor_position) = inline_document_with_cursor! {
            agent writer {
                <cursor>
            }
        };

        let (inference_source, inference_cursor_position) = inline_document_with_cursor! {
            agent writer {
                inference: {
                    <cursor>
                }
            }
        };

        let agent_document_state = DocumentState::new(agent_source);
        let inference_document_state = DocumentState::new(inference_source);

        let agent_completions = agent_document_state.completion_suggestions(agent_cursor_position);
        let inference_completions = inference_document_state.completion_suggestions(inference_cursor_position);

        let model_completion = agent_completions
            .iter()
            .find(|completion_suggestion| completion_suggestion.label == "model")
            .expect("agent completion should include model property");

        let max_tokens_completion = inference_completions
            .iter()
            .find(|completion_suggestion| completion_suggestion.label == InferenceSetting::MaxTokens.key())
            .expect("inference completion should include max_tokens setting");

        assert_eq!(model_completion.detail, "Model binding (required)");
        assert_eq!(max_tokens_completion.detail, "Token budget (integer)");
    }

    #[test]
    fn completes_registered_provider_models_inside_model_call() {
        let (source, cursor_position) = inline_document_with_cursor! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini", "gpt-4o-mini"]
            }

            agent writer {
                model: openai("<cursor>")
                prompt: "hello"
                output: string
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "gpt-4.1-mini", "gpt-4o-mini");
    }

    #[test]
    fn completes_schema_references_in_type_context() {
        let (source, cursor_position) = inline_document_with_cursor! {
            schema Person {
                name: string
            }

            input {
                profile: schema.<cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "Person");
    }

    #[test]
    fn excludes_current_schema_from_schema_type_suggestions() {
        let (source, cursor_position) = inline_document_with_cursor! {
            schema Person {
                related: schema.<cursor>
            }

            schema Team {
                members: [string]
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains!(&completion_suggestions, "Team");
        assert_completion_excludes_labels!(&completion_suggestions, "Person");
    }

    #[test]
    fn suppresses_key_suggestions_inside_input_block() {
        let (source, cursor_position) = inline_document_with_cursor! {
            input {
                <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert!(completion_suggestions.is_empty());
    }

    #[test]
    fn suggests_only_types_for_input_field_values() {
        let (source, cursor_position) = inline_document_with_cursor! {
            input {
                product_name: <cursor>
            }
        };

        let document_state = DocumentState::new(source);
        let completion_suggestions = document_state.completion_suggestions(cursor_position);

        assert_completion_contains_labels!(&completion_suggestions, TypeExpression::String, TypeExpression::Number);
        assert_completion_excludes_labels!(&completion_suggestions, DeclarationKeyword::Provider, DeclarationKeyword::Agent);
    }
}
