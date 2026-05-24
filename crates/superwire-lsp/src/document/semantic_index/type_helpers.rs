use std::collections::BTreeMap;

use lsp_types::CompletionItemKind;
use superwire_core::semantic::ToolingReferencePath;
use superwire_dsl::TypeExpression;

use super::super::reference::ReferenceCompletionPath;
use super::super::text_utils::trailing_identifier;
use super::super::{type_symbol_suggestions, CompletionSuggestion, RenderTypeExpression};
use super::types::SemanticIndex;

impl SemanticIndex {
    pub fn type_suggestions(&self, line_prefix: &str, current_schema_name: Option<&str>) -> Vec<CompletionSuggestion> {
        let trimmed_line_prefix = line_prefix.trim_end();

        if let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(line_prefix) {
            if reference_completion_path.is_schema_root() {
                return self.schema_type_reference_suggestions(&reference_completion_path, current_schema_name);
            }
        }

        if trimmed_line_prefix.ends_with('.') && !trimmed_line_prefix.ends_with("schema.") {
            return Vec::new();
        }

        if trimmed_line_prefix.ends_with("schema.") {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema".to_string(),
                    documentation: "Type declared in a `schema` block.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let prefix = trailing_identifier(line_prefix).unwrap_or_default();
        let mut completion_suggestions = type_symbol_suggestions()
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(prefix))
            .collect::<Vec<_>>();

        completion_suggestions.extend(self.structural_type_suggestions(prefix));

        completion_suggestions.extend(
            self.schema_names
                .iter()
                .filter(|schema_name| {
                    if current_schema_name == Some(schema_name.as_str()) {
                        return false;
                    }

                    let schema_reference = format!("schema.{schema_name}");
                    schema_reference.starts_with(prefix)
                })
                .map(|schema_name| CompletionSuggestion {
                    label: format!("schema.{schema_name}"),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema reference".to_string(),
                    documentation: "Reference a named schema type.".to_string(),
                    insert_text: format!("schema.{schema_name}"),
                }),
        );

        completion_suggestions
    }

    fn schema_type_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        current_schema_name: Option<&str>,
    ) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .filter(|schema_name| schema_name.starts_with(&reference_completion_path.pending_prefix))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema".to_string(),
                    documentation: "Type declared in a `schema` block.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let schema_name = &reference_completion_path.complete_accesses[0];

        if current_schema_name == Some(schema_name.as_str()) {
            return Vec::new();
        }

        let remaining_accesses = reference_completion_path.complete_accesses[1..].to_vec();
        let candidate_types = self
            .tooling_snapshot
            .resolve_reference_path_types(&ToolingReferencePath::schema(schema_name.clone(), remaining_accesses));
        let mut available_fields = BTreeMap::<String, TypeExpression>::new();

        for candidate_type in candidate_types {
            candidate_type.collect_available_field_types(
                &mut |schema_name| self.tooling_snapshot.schema_object_type(schema_name),
                &mut available_fields,
            );
        }

        available_fields
            .into_iter()
            .filter(|(field_name, field_type)| {
                field_name.starts_with(&reference_completion_path.pending_prefix) && field_type.is_string_enum_expression()
            })
            .map(|(field_name, field_type)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionItemKind::PROPERTY,
                detail: format!("Enum field: {}", field_type.render_type()),
                documentation: "Schema field usable as an enum reference.".to_string(),
                insert_text: field_name,
            })
            .collect()
    }

    fn structural_type_suggestions(&self, type_prefix: &str) -> Vec<CompletionSuggestion> {
        let structural_type_suggestions = [
            CompletionSuggestion {
                label: "[string]".to_string(),
                kind: CompletionItemKind::STRUCT,
                detail: "Array type".to_string(),
                documentation: "Array type expression.".to_string(),
                insert_text: "[string]".to_string(),
            },
            CompletionSuggestion {
                label: "{}".to_string(),
                kind: CompletionItemKind::STRUCT,
                detail: "Object type".to_string(),
                documentation: "Object type expression.".to_string(),
                insert_text: "{}".to_string(),
            },
        ];

        structural_type_suggestions
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(type_prefix))
            .collect()
    }
}
