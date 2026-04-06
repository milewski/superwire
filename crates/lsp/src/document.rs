use std::collections::HashSet;

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
    CodeLensHint, CompletionKind, CompletionSuggestion, DiagnosticSeverity, DocumentDiagnostic, DocumentFormattingEdit, DocumentSymbolNode,
    FoldingRangeBlock, SymbolKind, WorkspaceSymbolMatch,
};

use superwire_core::dsl::TypeExpression;
use superwire_core::runtime::ProviderDriver;

use crate::protocol::Position;

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

    fn line_prefix(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let cursor_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().take(cursor_index).collect())
    }

    fn symbol_token_at(&self, position: Position) -> Option<String> {
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

fn primitive_type_expressions() -> [TypeExpression; 5] {
    [
        TypeExpression::String,
        TypeExpression::Number,
        TypeExpression::Float,
        TypeExpression::Boolean,
        TypeExpression::Null,
    ]
}

#[cfg(test)]
mod tests;
