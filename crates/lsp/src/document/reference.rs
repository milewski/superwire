use std::collections::BTreeMap;

use engine_ai_core::dsl::{DeclarationKeyword, ForClauseKeyword, ReferenceKeyword, SourcePosition, SourceSpan, TypeExpression, TypedField};

use crate::protocol::Position;

use super::semantic_index::SemanticIndex;
use super::{CompletionKind, CompletionSuggestion, RenderTypeExpression};

#[derive(Debug, Clone)]
pub(super) struct ReferenceCompletionPath {
    root: String,
    pub(super) complete_accesses: Vec<String>,
    pub(super) pending_prefix: String,
}

impl ReferenceCompletionPath {
    pub(super) fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let reference_token = trailing_reference_token(line_prefix)?;

        Self::from_token(reference_token)
    }

    pub(super) fn from_token(reference_token: &str) -> Option<Self> {
        if reference_token.is_empty() || reference_token.ends_with('?') {
            return None;
        }

        let normalized_token = reference_token.replace("?.", ".");

        if normalized_token.contains("..") {
            return None;
        }

        let token_parts = normalized_token.split('.').collect::<Vec<_>>();
        let root = (*token_parts.first()?).to_string();

        if !is_identifier(root.as_str()) {
            return None;
        }

        let token_has_trailing_separator = normalized_token.ends_with('.');

        if token_parts.len() == 1 {
            return Some(Self {
                root,
                complete_accesses: Vec::new(),
                pending_prefix: String::new(),
            });
        }

        let mut complete_accesses = Vec::<String>::new();

        if token_has_trailing_separator {
            for token_part in token_parts.iter().skip(1).take(token_parts.len().saturating_sub(2)) {
                if token_part.is_empty() || !is_identifier(token_part) {
                    return None;
                }

                complete_accesses.push((*token_part).to_string());
            }

            return Some(Self {
                root,
                complete_accesses,
                pending_prefix: String::new(),
            });
        }

        for token_part in token_parts.iter().skip(1).take(token_parts.len().saturating_sub(2)) {
            if token_part.is_empty() || !is_identifier(token_part) {
                return None;
            }

            complete_accesses.push((*token_part).to_string());
        }

        let pending_prefix = (*token_parts.last()?).to_string();

        if !pending_prefix.is_empty() && !is_identifier(&pending_prefix) {
            return None;
        }

        Some(Self {
            root,
            complete_accesses,
            pending_prefix,
        })
    }

    pub(super) fn root_keyword(&self) -> Option<ReferenceKeyword> {
        ReferenceKeyword::from_identifier(&self.root)
    }

    fn root_declaration_keyword(&self) -> Option<DeclarationKeyword> {
        DeclarationKeyword::from_identifier(&self.root)
    }

    pub(super) fn is_schema_root(&self) -> bool {
        self.root_declaration_keyword() == Some(DeclarationKeyword::Schema)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReferenceCompletionConstraint {
    None,
    ForLoopIterable,
}

impl ReferenceCompletionConstraint {
    pub(super) fn from_line_prefix(line_prefix: &str) -> Self {
        if is_for_loop_iterable_reference_context(line_prefix) {
            return Self::ForLoopIterable;
        }

        Self::None
    }
}

impl SemanticIndex {
    pub(super) fn reference_path_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        position: Position,
    ) -> Vec<CompletionSuggestion> {
        let current_schema_name = self.schema_name_at_position(position);
        let current_agent_name = self.agent_name_at_position(position);

        if reference_completion_path.is_schema_root() {
            if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
                return Vec::new();
            }

            return self.schema_reference_suggestions(reference_completion_path, current_schema_name);
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input) => self.singleton_reference_suggestions(
                &self.input_fields,
                &reference_completion_path.complete_accesses,
                &reference_completion_path.pending_prefix,
                "Input field",
                reference_completion_constraint,
            ),
            Some(ReferenceKeyword::Secrets) => self.singleton_reference_suggestions(
                &self.secrets_fields,
                &reference_completion_path.complete_accesses,
                &reference_completion_path.pending_prefix,
                "Secrets field",
                reference_completion_constraint,
            ),
            Some(ReferenceKeyword::Agent) => {
                self.agent_reference_suggestions(reference_completion_path, reference_completion_constraint, current_agent_name)
            }
            Some(ReferenceKeyword::Tool) | None => Vec::new(),
        }
    }

    pub(super) fn resolve_singleton_reference_type(
        &self,
        root_fields: &BTreeMap<String, TypeExpression>,
        resolved_accesses: &[String],
    ) -> Option<TypeExpression> {
        let first_field_name = resolved_accesses.first()?;
        let root_field_type = root_fields.get(first_field_name)?.clone();

        if resolved_accesses.len() == 1 {
            return Some(root_field_type);
        }

        let candidate_types = self.resolve_access_path(vec![root_field_type], &resolved_accesses[1..]);

        candidate_types.first().cloned()
    }

    fn singleton_reference_suggestions(
        &self,
        root_fields: &BTreeMap<String, TypeExpression>,
        complete_accesses: &[String],
        pending_prefix: &str,
        detail_prefix: &str,
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> Vec<CompletionSuggestion> {
        if complete_accesses.is_empty() {
            return root_fields
                .iter()
                .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
                .filter(|(_, field_type)| {
                    reference_completion_constraint == ReferenceCompletionConstraint::None || field_type.supports_for_loop_iterable()
                })
                .map(|(field_name, field_type)| CompletionSuggestion {
                    label: field_name.clone(),
                    kind: CompletionKind::Property,
                    detail: format!("{detail_prefix}: {}", field_type.render_type()),
                    documentation: "Field in singleton declaration.".to_string(),
                    insert_text: field_name.clone(),
                })
                .collect();
        }

        let first_field_name = &complete_accesses[0];
        let Some(root_field_type) = root_fields.get(first_field_name).cloned() else {
            return Vec::new();
        };

        let candidate_types = self.resolve_access_path(vec![root_field_type], &complete_accesses[1..]);

        self.field_suggestions_from_types(candidate_types.as_slice(), pending_prefix, reference_completion_constraint)
    }

    fn agent_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        current_agent_name: Option<&str>,
    ) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .agent_names
                .iter()
                .filter(|agent_name| agent_name.starts_with(&reference_completion_path.pending_prefix))
                .filter(|agent_name| current_agent_name.is_none_or(|current_name| *agent_name != current_name))
                .filter(|agent_name| {
                    if reference_completion_constraint == ReferenceCompletionConstraint::None {
                        return true;
                    }

                    let Some(agent_summary) = self.agents.get(*agent_name) else {
                        return false;
                    };

                    let Some(agent_output_type) = &agent_summary.output_type else {
                        return false;
                    };

                    agent_output_type.supports_for_loop_iterable()
                })
                .map(|agent_name| CompletionSuggestion {
                    label: agent_name.clone(),
                    kind: CompletionKind::Variable,
                    detail: "Declared agent".to_string(),
                    documentation: "Reference to a declared agent output.".to_string(),
                    insert_text: agent_name.clone(),
                })
                .collect();
        }

        let agent_name = &reference_completion_path.complete_accesses[0];

        if current_agent_name == Some(agent_name.as_str()) {
            return Vec::new();
        }

        let Some(agent_summary) = self.agents.get(agent_name) else {
            return Vec::new();
        };

        let Some(agent_output_type) = agent_summary.output_type.clone() else {
            return Vec::new();
        };

        let remaining_accesses = &reference_completion_path.complete_accesses[1..];
        let candidate_types = self.resolve_access_path(vec![agent_output_type], remaining_accesses);

        self.field_suggestions_from_types(
            candidate_types.as_slice(),
            &reference_completion_path.pending_prefix,
            reference_completion_constraint,
        )
    }

    fn schema_reference_suggestions(
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
                    kind: CompletionKind::Type,
                    detail: "Named schema".to_string(),
                    documentation: "Named schema type from this workflow.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let schema_name = &reference_completion_path.complete_accesses[0];

        if current_schema_name == Some(schema_name.as_str()) {
            return Vec::new();
        }

        let Some(schema_summary) = self.schemas.get(schema_name) else {
            return Vec::new();
        };

        let root_type = TypeExpression::Object(
            schema_summary
                .fields
                .iter()
                .map(|(field_name, field_type)| TypedField {
                    name: field_name.clone(),
                    field_type: field_type.clone(),
                    description: None,
                    span: SourceSpan {
                        start: SourcePosition { line: 1, column: 1 },
                        end: SourcePosition { line: 1, column: 1 },
                    },
                })
                .collect(),
        );

        let candidate_types = self.resolve_access_path(vec![root_type], &reference_completion_path.complete_accesses[1..]);

        self.field_suggestions_from_types(
            candidate_types.as_slice(),
            &reference_completion_path.pending_prefix,
            ReferenceCompletionConstraint::None,
        )
    }

    fn field_suggestions_from_types(
        &self,
        candidate_types: &[TypeExpression],
        pending_prefix: &str,
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> Vec<CompletionSuggestion> {
        let mut available_fields = BTreeMap::<String, TypeExpression>::new();

        for candidate_type in candidate_types {
            self.collect_available_fields(candidate_type, &mut available_fields);
        }

        available_fields
            .into_iter()
            .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
            .filter(|(_, field_type)| {
                reference_completion_constraint == ReferenceCompletionConstraint::None || field_type.supports_for_loop_iterable()
            })
            .map(|(field_name, field_type)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionKind::Property,
                detail: format!("Field: {}", field_type.render_type()),
                documentation: "Field available at this reference path.".to_string(),
                insert_text: field_name,
            })
            .collect()
    }

    pub(in crate::document) fn resolve_access_path(&self, start_types: Vec<TypeExpression>, accesses: &[String]) -> Vec<TypeExpression> {
        let mut candidate_types = start_types;

        for access_name in accesses {
            let mut next_candidate_types = Vec::<TypeExpression>::new();

            for candidate_type in &candidate_types {
                self.collect_next_types_for_field(candidate_type, access_name, &mut next_candidate_types);
            }

            if next_candidate_types.is_empty() {
                return Vec::new();
            }

            candidate_types = next_candidate_types;
        }

        candidate_types
    }

    fn collect_next_types_for_field(
        &self,
        candidate_type: &TypeExpression,
        field_name: &str,
        next_candidate_types: &mut Vec<TypeExpression>,
    ) {
        match candidate_type {
            TypeExpression::Object(typed_fields) => {
                if let Some(typed_field) = typed_fields.iter().find(|typed_field| typed_field.name == field_name) {
                    next_candidate_types.push(typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                let Some(schema_summary) = self.schemas.get(schema_name) else {
                    return;
                };

                if let Some(field_type) = schema_summary.fields.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    self.collect_next_types_for_field(union_member, field_name, next_candidate_types);
                }
            }
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::StringEnum(_) => {}
        }
    }

    fn collect_available_fields(&self, candidate_type: &TypeExpression, available_fields: &mut BTreeMap<String, TypeExpression>) {
        match candidate_type {
            TypeExpression::Object(typed_fields) => {
                for typed_field in typed_fields {
                    available_fields
                        .entry(typed_field.name.clone())
                        .or_insert_with(|| typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                let Some(schema_summary) = self.schemas.get(schema_name) else {
                    return;
                };

                for (field_name, field_type) in &schema_summary.fields {
                    available_fields.entry(field_name.clone()).or_insert_with(|| field_type.clone());
                }
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    self.collect_available_fields(union_member, available_fields);
                }
            }
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::StringEnum(_) => {}
        }
    }
}

trait ForLoopIterableType {
    fn supports_for_loop_iterable(&self) -> bool;
}

impl ForLoopIterableType for TypeExpression {
    fn supports_for_loop_iterable(&self) -> bool {
        match self {
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            } => true,
            TypeExpression::Union(union_members) => union_members.iter().any(ForLoopIterableType::supports_for_loop_iterable),
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_) => false,
        }
    }
}

fn trailing_reference_token(line_prefix: &str) -> Option<&str> {
    let mut start_index = line_prefix.len();

    for (character_index, character) in line_prefix.char_indices().rev() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?' {
            start_index = character_index;
            continue;
        }

        break;
    }

    if start_index == line_prefix.len() {
        return None;
    }

    Some(&line_prefix[start_index..])
}

fn is_for_loop_iterable_reference_context(line_prefix: &str) -> bool {
    let for_keyword = ForClauseKeyword::For.as_str();
    let in_keyword = ForClauseKeyword::In.as_str();
    let for_keyword_with_surrounding_whitespace = format!(" {for_keyword} ");
    let for_keyword_with_trailing_whitespace = format!("{for_keyword} ");

    let Some(reference_token) = trailing_reference_token(line_prefix) else {
        return false;
    };

    let Some(reference_start_index) = line_prefix.rfind(reference_token) else {
        return false;
    };

    let prefix_before_reference = &line_prefix[..reference_start_index];
    let for_clause_prefix = prefix_before_reference
        .rfind(for_keyword_with_surrounding_whitespace.as_str())
        .map_or(prefix_before_reference, |for_clause_index| {
            &prefix_before_reference[for_clause_index + 1..]
        });

    let Some(after_for_keyword) = for_clause_prefix.strip_prefix(for_keyword_with_trailing_whitespace.as_str()) else {
        return false;
    };

    let Some(iterator_name) = leading_identifier(after_for_keyword) else {
        return false;
    };

    let remaining_after_iterator = after_for_keyword[iterator_name.len()..].trim_start();
    let Some(after_in_keyword) = remaining_after_iterator.strip_prefix(in_keyword) else {
        return false;
    };

    after_in_keyword.starts_with(char::is_whitespace)
}

fn leading_identifier(source_text: &str) -> Option<&str> {
    let mut identifier_end = 0;

    for character in source_text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identifier_end += character.len_utf8();
            continue;
        }

        break;
    }

    if identifier_end == 0 {
        return None;
    }

    let identifier = &source_text[..identifier_end];

    if !is_identifier(identifier) {
        return None;
    }

    Some(identifier)
}

fn is_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };

    if !first_character.is_ascii_alphabetic() && first_character != '_' {
        return false;
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}
