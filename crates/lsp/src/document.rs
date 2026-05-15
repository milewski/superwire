use std::collections::HashSet;

mod code_action;
mod completion;
mod completion_context;
mod definition;
mod folding;
mod formatting;
mod hover;
mod position;
mod reference;
mod scope;
mod semantic_index;
mod snapshot;
mod symbol;
mod text_utils;
mod types;

use snapshot::SemanticSnapshot;
use text_utils::is_symbol_character;
pub use types::{
    CodeActionEdit, CodeActionSuggestion, CodeLensHint, CompletionSuggestion, DocumentDiagnostic, DocumentFormattingEdit,
    DocumentSymbolNode, FoldingRangeBlock, WorkspaceSymbolMatch,
};

use lsp_types::{CompletionItemKind, DiagnosticSeverity, Position};
use superwire_core::dsl::{parse_workflow, Declaration, Expression, ToolPropertyName, TypeExpression, TypedField};
use superwire_core::mcp::{McpLock, McpToolLock};
use superwire_core::semantic::ProviderDriver;

use crate::diagnostic_code::DiagnosticCode;

#[derive(Debug)]
pub struct DocumentState {
    text: String,
    semantic_snapshot: SemanticSnapshot,
}

#[derive(Debug)]
pub(super) struct SymbolTokenAtPosition {
    pub symbol_token: String,
    pub cursor_character_offset: usize,
}

impl DocumentState {
    #[must_use]
    pub fn new(text: String, mcp_lock: Option<McpLock>) -> Self {
        let semantic_snapshot = SemanticSnapshot::from_text(&text, mcp_lock.as_ref());

        Self { text, semantic_snapshot }
    }

    pub fn replace_text(&mut self, text: String, mcp_lock: Option<McpLock>) {
        self.semantic_snapshot = SemanticSnapshot::from_text(&text, mcp_lock.as_ref());
        self.text = text;
    }

    #[must_use]
    pub(super) fn mcp_lock(&self) -> Option<McpLock> {
        self.semantic_snapshot.semantic_index.mcp_lock.clone()
    }

    #[must_use]
    pub fn diagnostics(&self) -> Vec<DocumentDiagnostic> {
        let mut diagnostics = self.semantic_snapshot.diagnostics(&self.text);
        diagnostics.extend(self.mcp_schema_diagnostics());

        diagnostics
    }

    #[allow(clippy::too_many_lines)]
    fn mcp_schema_diagnostics(&self) -> Vec<DocumentDiagnostic> {
        let Some(mcp_lock) = self.semantic_snapshot.semantic_index.mcp_lock.as_ref() else {
            return Vec::new();
        };
        let Ok(workflow) = parse_workflow(&self.text) else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::Tool(tool_declaration) => {
                    if let Some(superwire_core::dsl::ToolSource::Mcp(mcp_tool_source)) = &tool_declaration.source {
                        if let Some(mcp_tool_lock) =
                            Self::mcp_tool_lock(mcp_lock, mcp_tool_source.server_name.as_deref(), &mcp_tool_source.tool_name)
                        {
                            diagnostics.extend(self.tool_schema_override_diagnostics(
                                &tool_declaration.name,
                                mcp_tool_lock,
                                &tool_declaration.input_fields,
                                ToolPropertyName::Input,
                            ));
                            diagnostics.extend(self.binding_override_diagnostics(
                                &tool_declaration.name,
                                mcp_tool_lock,
                                &tool_declaration.fixed_binding_fields,
                            ));
                            diagnostics.extend(self.tool_schema_override_diagnostics(
                                &tool_declaration.name,
                                mcp_tool_lock,
                                &tool_declaration.output_fields,
                                ToolPropertyName::Output,
                            ));
                        }
                    }
                }
                Declaration::McpToolBatch(mcp_tool_batch) => {
                    let Some(server_lock) = mcp_lock.servers.get(&mcp_tool_batch.server_name) else {
                        continue;
                    };
                    let tool_locks = mcp_tool_batch
                        .items
                        .iter()
                        .filter_map(|item| {
                            server_lock
                                .find_tool_with_name(&item.source_name)
                                .map(|(_resolved_tool_name, tool_lock)| (item.local_name.as_str(), tool_lock))
                        })
                        .collect::<Vec<_>>();

                    diagnostics.extend(self.batch_common_schema_diagnostics(
                        &tool_locks,
                        &mcp_tool_batch.input_fields,
                        ToolPropertyName::Input,
                    ));
                    diagnostics.extend(self.batch_common_schema_diagnostics(
                        &tool_locks,
                        &mcp_tool_batch.output_fields,
                        ToolPropertyName::Output,
                    ));
                    diagnostics.extend(self.batch_binding_override_diagnostics(&tool_locks, &mcp_tool_batch.fixed_binding_fields));

                    for item in &mcp_tool_batch.items {
                        let Some((_, mcp_tool_lock)) = server_lock.find_tool_with_name(&item.source_name) else {
                            continue;
                        };

                        diagnostics.extend(self.tool_schema_override_diagnostics(
                            &item.local_name,
                            mcp_tool_lock,
                            &item.input_fields,
                            ToolPropertyName::Input,
                        ));
                        diagnostics.extend(self.binding_override_diagnostics(&item.local_name, mcp_tool_lock, &item.fixed_binding_fields));
                        diagnostics.extend(self.tool_schema_override_diagnostics(
                            &item.local_name,
                            mcp_tool_lock,
                            &item.output_fields,
                            ToolPropertyName::Output,
                        ));
                    }
                }
                Declaration::McpBatch(mcp_batch) => {
                    let Some(server_lock) = mcp_lock.servers.get(&mcp_batch.server_name) else {
                        continue;
                    };
                    let tool_locks = mcp_batch
                        .tool_items
                        .iter()
                        .filter_map(|item| {
                            server_lock
                                .find_tool_with_name(&item.source_name)
                                .map(|(_resolved_tool_name, tool_lock)| (item.local_name.as_str(), tool_lock))
                        })
                        .collect::<Vec<_>>();

                    diagnostics.extend(self.batch_common_schema_diagnostics(&tool_locks, &mcp_batch.input_fields, ToolPropertyName::Input));
                    diagnostics.extend(self.batch_common_schema_diagnostics(
                        &tool_locks,
                        &mcp_batch.output_fields,
                        ToolPropertyName::Output,
                    ));
                    diagnostics.extend(self.batch_binding_override_diagnostics(&tool_locks, &mcp_batch.fixed_binding_fields));

                    for item in &mcp_batch.tool_items {
                        let Some((_, mcp_tool_lock)) = server_lock.find_tool_with_name(&item.source_name) else {
                            continue;
                        };

                        diagnostics.extend(self.tool_schema_override_diagnostics(
                            &item.local_name,
                            mcp_tool_lock,
                            &item.input_fields,
                            ToolPropertyName::Input,
                        ));
                        diagnostics.extend(self.binding_override_diagnostics(&item.local_name, mcp_tool_lock, &item.fixed_binding_fields));
                        diagnostics.extend(self.tool_schema_override_diagnostics(
                            &item.local_name,
                            mcp_tool_lock,
                            &item.output_fields,
                            ToolPropertyName::Output,
                        ));
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }

    fn mcp_tool_lock<'lock>(mcp_lock: &'lock McpLock, server_name: Option<&str>, tool_name: &str) -> Option<&'lock McpToolLock> {
        if let Some(server_name) = server_name {
            let server_lock = mcp_lock.servers.get(server_name)?;

            return server_lock
                .find_tool_with_name(tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock);
        }

        mcp_lock.servers.values().find_map(|server_lock| {
            server_lock
                .find_tool_with_name(tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock)
        })
    }

    fn tool_schema_override_diagnostics(
        &self,
        tool_name: &str,
        mcp_tool_lock: &McpToolLock,
        typed_fields: &[TypedField],
        property_name: ToolPropertyName,
    ) -> Vec<DocumentDiagnostic> {
        let expected_fields = Self::mcp_schema_fields(mcp_tool_lock, property_name);

        typed_fields
            .iter()
            .filter_map(|typed_field| {
                let expected_field = expected_fields
                    .iter()
                    .find(|expected_field| expected_field.name == typed_field.name);
                let message = Self::schema_field_validation_message(tool_name, property_name, typed_field, expected_field)?;

                Some(self.mcp_schema_diagnostic(typed_field.span, message))
            })
            .collect()
    }

    fn batch_common_schema_diagnostics(
        &self,
        tool_locks: &[(&str, &McpToolLock)],
        typed_fields: &[TypedField],
        property_name: ToolPropertyName,
    ) -> Vec<DocumentDiagnostic> {
        let mut diagnostics = Vec::new();

        for typed_field in typed_fields {
            for (tool_name, mcp_tool_lock) in tool_locks {
                let expected_fields = Self::mcp_schema_fields(mcp_tool_lock, property_name);
                let expected_field = expected_fields
                    .iter()
                    .find(|expected_field| expected_field.name == typed_field.name);
                let Some(message) = Self::schema_field_validation_message(tool_name, property_name, typed_field, expected_field) else {
                    continue;
                };

                diagnostics.push(self.mcp_schema_diagnostic(typed_field.span, message));
            }
        }

        diagnostics
    }

    fn schema_field_validation_message(
        tool_name: &str,
        property_name: ToolPropertyName,
        typed_field: &TypedField,
        expected_field: Option<&TypedField>,
    ) -> Option<String> {
        let Some(expected_field) = expected_field else {
            return Some(format!(
                "MCP tool `{tool_name}` has no `{}` field `{}` in the lock file.",
                property_name.as_str(),
                typed_field.name
            ));
        };

        if Self::type_expressions_match(&expected_field.field_type, &typed_field.field_type) {
            return None;
        }

        Some(format!(
            "MCP tool `{tool_name}` `{}` field `{}` must be `{}`, found `{}`.",
            property_name.as_str(),
            typed_field.name,
            expected_field.field_type.render_type(),
            typed_field.field_type.render_type()
        ))
    }

    fn binding_override_diagnostics(
        &self,
        tool_name: &str,
        mcp_tool_lock: &McpToolLock,
        binding_fields: &[superwire_core::dsl::ObjectField],
    ) -> Vec<DocumentDiagnostic> {
        let expected_fields = Self::mcp_schema_fields(mcp_tool_lock, ToolPropertyName::Input);

        binding_fields
            .iter()
            .filter_map(|binding_field| {
                let expected_field = expected_fields
                    .iter()
                    .find(|expected_field| expected_field.name == binding_field.name);
                let message = Self::binding_field_validation_message(tool_name, &binding_field.name, &binding_field.value, expected_field)?;

                Some(self.mcp_schema_diagnostic(binding_field.span, message))
            })
            .collect()
    }

    fn batch_binding_override_diagnostics(
        &self,
        tool_locks: &[(&str, &McpToolLock)],
        binding_fields: &[superwire_core::dsl::ObjectField],
    ) -> Vec<DocumentDiagnostic> {
        let mut diagnostics = Vec::new();

        for binding_field in binding_fields {
            for (tool_name, mcp_tool_lock) in tool_locks {
                let expected_fields = Self::mcp_schema_fields(mcp_tool_lock, ToolPropertyName::Input);
                let expected_field = expected_fields
                    .iter()
                    .find(|expected_field| expected_field.name == binding_field.name);
                let Some(message) =
                    Self::binding_field_validation_message(tool_name, &binding_field.name, &binding_field.value, expected_field)
                else {
                    continue;
                };

                diagnostics.push(self.mcp_schema_diagnostic(binding_field.span, message));
            }
        }

        diagnostics
    }

    fn binding_field_validation_message(
        tool_name: &str,
        field_name: &str,
        value: &Expression,
        expected_field: Option<&TypedField>,
    ) -> Option<String> {
        let Some(expected_field) = expected_field else {
            return Some(format!(
                "MCP tool `{tool_name}` has no input field `{field_name}` in the lock file."
            ));
        };

        if Self::literal_matches_type(value, &expected_field.field_type) {
            return None;
        }

        Some(format!(
            "MCP tool `{tool_name}` binding `{field_name}` must be `{}`.",
            expected_field.field_type.render_type()
        ))
    }

    fn mcp_schema_fields(mcp_tool_lock: &McpToolLock, property_name: ToolPropertyName) -> Vec<TypedField> {
        match property_name {
            ToolPropertyName::Input | ToolPropertyName::Bindings => mcp_tool_lock.input_fields_except(&[]),
            ToolPropertyName::Output => mcp_tool_lock.output_fields(),
            ToolPropertyName::Description | ToolPropertyName::MaxCalls => Vec::new(),
        }
    }

    fn literal_matches_type(value: &Expression, expected_type: &TypeExpression) -> bool {
        match (value, expected_type) {
            (Expression::StringLiteral(_), TypeExpression::String | TypeExpression::StringEnum(_))
            | (Expression::NumberLiteral(_), TypeExpression::Number | TypeExpression::Float)
            | (Expression::BooleanLiteral(_), TypeExpression::Boolean)
            | (Expression::NullLiteral, TypeExpression::Null) => true,
            (expression, TypeExpression::Union(type_expressions)) => type_expressions
                .iter()
                .any(|type_expression| Self::literal_matches_type(expression, type_expression)),
            (Expression::StringTemplate(_), TypeExpression::String)
            | (Expression::Reference(_) | Expression::FunctionCall(_) | Expression::ToolCall(_) | Expression::McpCall(_), _)
            | (Expression::ArrayLiteral(_), TypeExpression::Array { .. } | TypeExpression::Tuple(_))
            | (Expression::ObjectLiteral(_), TypeExpression::Object(_) | TypeExpression::AnyObject | TypeExpression::Variant { .. }) => {
                true
            }
            _ => false,
        }
    }

    fn type_expressions_match(expected_type: &TypeExpression, found_type: &TypeExpression) -> bool {
        match (expected_type, found_type) {
            (TypeExpression::String, TypeExpression::String)
            | (TypeExpression::Number, TypeExpression::Number)
            | (TypeExpression::Float, TypeExpression::Float)
            | (TypeExpression::Boolean, TypeExpression::Boolean)
            | (TypeExpression::Null, TypeExpression::Null)
            | (TypeExpression::AnyObject, TypeExpression::AnyObject) => true,
            (TypeExpression::SchemaReference(expected_schema), TypeExpression::SchemaReference(found_schema)) => {
                expected_schema == found_schema
            }
            (TypeExpression::StringEnum(expected_value), TypeExpression::StringEnum(found_value)) => expected_value == found_value,
            (TypeExpression::StringEnumReference(expected_reference), TypeExpression::StringEnumReference(found_reference)) => {
                expected_reference == found_reference
            }
            (
                TypeExpression::Array {
                    item_type: expected_item_type,
                    fixed_length: expected_fixed_length,
                },
                TypeExpression::Array {
                    item_type: found_item_type,
                    fixed_length: found_fixed_length,
                },
            ) => {
                expected_fixed_length == found_fixed_length
                    && Self::type_expressions_match(expected_item_type.as_ref(), found_item_type.as_ref())
            }
            (TypeExpression::Tuple(expected_items), TypeExpression::Tuple(found_items))
            | (TypeExpression::Union(expected_items), TypeExpression::Union(found_items)) => {
                expected_items.len() == found_items.len()
                    && expected_items
                        .iter()
                        .zip(found_items)
                        .all(|(expected_item, found_item)| Self::type_expressions_match(expected_item, found_item))
            }
            (TypeExpression::Object(expected_fields), TypeExpression::Object(found_fields)) => {
                expected_fields.len() == found_fields.len()
                    && expected_fields.iter().all(|expected_field| {
                        found_fields
                            .iter()
                            .find(|found_field| found_field.name == expected_field.name)
                            .is_some_and(|found_field| Self::type_expressions_match(&expected_field.field_type, &found_field.field_type))
                    })
            }
            (
                TypeExpression::Variant {
                    discriminator: expected_discriminator,
                    cases: expected_cases,
                },
                TypeExpression::Variant {
                    discriminator: found_discriminator,
                    cases: found_cases,
                },
            ) => expected_discriminator == found_discriminator && expected_cases == found_cases,
            _ => false,
        }
    }

    fn mcp_schema_diagnostic(&self, span: superwire_core::dsl::SourceSpan, message: String) -> DocumentDiagnostic {
        DocumentDiagnostic {
            range: position::source_span_to_range(&self.text, span),
            severity: DiagnosticSeverity::ERROR,
            code: DiagnosticCode::InvalidToolBinding,
            message,
        }
    }

    fn line_prefix(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let cursor_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().take(cursor_index).collect())
    }

    fn line_suffix(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let cursor_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().skip(cursor_index).collect())
    }

    fn symbol_token_at_position(&self, position: Position) -> Option<SymbolTokenAtPosition> {
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

        Some(SymbolTokenAtPosition {
            symbol_token: line_characters[start_index..end_index].iter().collect(),
            cursor_character_offset: cursor_index.saturating_sub(start_index),
        })
    }

    fn symbol_token_at(&self, position: Position) -> Option<String> {
        self.symbol_token_at_position(position)
            .map(|symbol_token_at_position| symbol_token_at_position.symbol_token)
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

trait RenderTypeExpression {
    fn render_type(&self) -> String;

    fn render_type_expanded(&self, indent: &str) -> String;
}

impl RenderTypeExpression for TypeExpression {
    fn render_type(&self) -> String {
        match self {
            Self::String => "string".to_string(),
            Self::Number => "number".to_string(),
            Self::Float => "float".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::Null => "null".to_string(),
            Self::AnyObject => "object".to_string(),
            Self::SchemaReference(schema_name) => format!("schema.{schema_name}"),
            Self::StringEnum(enum_value) => render_string_enum_case(enum_value),
            Self::StringEnumReference(enum_reference) => enum_reference.render_path(),
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
                if typed_fields.is_empty() {
                    return "{}".to_string();
                }

                let field_strings = typed_fields
                    .iter()
                    .map(|typed_field| format!("{}: {}", typed_field.name, typed_field.field_type.render_type()))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{{ {field_strings} }}")
            }
            Self::Variant { discriminator, cases } => {
                let case_names = cases
                    .iter()
                    .map(|variant_case| variant_case.name.clone())
                    .collect::<Vec<_>>()
                    .join(" | ");

                format!("variant {discriminator} {{ {case_names} }}")
            }
            Self::Union(union_members) => {
                if let Some(nullable_member) = nullable_union_member(union_members.as_slice()) {
                    return format!("maybe {}", nullable_member.render_type());
                }

                if union_members.iter().all(|union_member| matches!(union_member, Self::StringEnum(_))) {
                    let enum_values = union_members
                        .iter()
                        .filter_map(|union_member| match union_member {
                            Self::StringEnum(enum_value) => Some(enum_value.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();

                    return render_string_enum_type(enum_values.as_slice());
                }

                union_members
                    .iter()
                    .map(RenderTypeExpression::render_type)
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        }
    }

    fn render_type_expanded(&self, indent: &str) -> String {
        match self {
            Self::Array { item_type, fixed_length } => {
                if fixed_length.is_some() || !matches!(item_type.as_ref(), Self::Object(_)) {
                    return self.render_type();
                }

                if matches!(item_type.as_ref(), Self::Object(typed_fields) if typed_fields.is_empty()) {
                    return self.render_type();
                }

                let item_indent = format!("{indent}    ");
                let closing_indent = indent.to_string();

                format!("[\n{}\n{closing_indent}]", item_type.render_type_expanded(item_indent.as_str()))
            }
            Self::Object(typed_fields) => {
                if typed_fields.is_empty() {
                    return "{}".to_string();
                }

                let field_indent = format!("{indent}    ");
                let rendered_fields = typed_fields
                    .iter()
                    .map(|typed_field| {
                        let rendered_description = typed_field
                            .description
                            .as_ref()
                            .map(|description_text| {
                                description_text
                                    .lines()
                                    .map(|description_line| format!("{field_indent}/// {description_line}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default();

                        let rendered_field = format!(
                            "{field_indent}{}: {},",
                            typed_field.name,
                            typed_field.field_type.render_type_expanded(field_indent.as_str())
                        );

                        if rendered_description.is_empty() {
                            return rendered_field;
                        }

                        format!("{rendered_description}\n{rendered_field}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                format!("{indent}{{\n{rendered_fields}\n{indent}}}")
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Tuple(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            }
            | Self::Union(_) => self.render_type(),
        }
    }
}

fn nullable_union_member(union_members: &[TypeExpression]) -> Option<TypeExpression> {
    if !union_members
        .iter()
        .any(|union_member| matches!(union_member, TypeExpression::Null))
    {
        return None;
    }

    let non_null_members = union_members
        .iter()
        .filter(|union_member| !matches!(union_member, TypeExpression::Null))
        .cloned()
        .collect::<Vec<_>>();

    if non_null_members.len() == 1 {
        return non_null_members.into_iter().next();
    }

    if non_null_members
        .iter()
        .all(|non_null_member| matches!(non_null_member, TypeExpression::StringEnum(_)))
    {
        return Some(TypeExpression::Union(non_null_members));
    }

    None
}

fn render_string_enum_type(enum_values: &[String]) -> String {
    let rendered_enum_values = enum_values
        .iter()
        .map(|enum_value| render_string_enum_case(enum_value))
        .collect::<Vec<_>>()
        .join(", ");

    format!("enum {{ {rendered_enum_values} }}")
}

fn render_string_enum_case(enum_value: &str) -> String {
    if is_wire_identifier(enum_value) {
        return enum_value.to_string();
    }

    let escaped_enum_value = enum_value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped_enum_value}\"")
}

fn is_wire_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };

    if !(first_character.is_ascii_alphabetic() || first_character == '_') {
        return false;
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn type_symbol_suggestions() -> Vec<CompletionSuggestion> {
    primitive_type_expressions()
        .into_iter()
        .map(|primitive_type_expression| {
            let type_name = primitive_type_expression.render_type();

            CompletionSuggestion {
                label: type_name.clone(),
                kind: CompletionItemKind::STRUCT,
                detail: "Primitive type".to_string(),
                documentation: "Primitive workflow type.".to_string(),
                insert_text: type_name.clone(),
            }
        })
        .collect()
}

fn primitive_type_expressions() -> [TypeExpression; 5] {
    [
        TypeExpression::String,
        TypeExpression::Number,
        TypeExpression::Float,
        TypeExpression::Boolean,
        TypeExpression::AnyObject,
    ]
}

#[cfg(test)]
mod tests;
