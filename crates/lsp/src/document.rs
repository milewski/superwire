use std::collections::{BTreeMap, HashMap, HashSet};

mod scope;
mod snapshot;
mod types;

use scope::{agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions, CompletionScope};
use snapshot::SemanticSnapshot;
pub use types::{CompletionKind, CompletionSuggestion, DiagnosticSeverity, DocumentDiagnostic};

use engine_ai_core::dsl::{
    parse_workflow, AgentProperty, Declaration, DeclarationKeyword, Expression, ForClauseKeyword, ProviderDeclaration, ReferenceKeyword,
    SingletonDeclarationKind, SourcePosition, SourceSpan, TypeExpression, TypedField, Workflow,
};
use engine_ai_core::runtime::ProviderDriver;

use crate::protocol::{Position, Range};

const COMPLETION_RECOVERY_PLACEHOLDER: &str = "__completion_placeholder";

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
    pub fn completion_suggestions(&self, position: Position) -> Vec<CompletionSuggestion> {
        let Some(line_prefix) = self.line_prefix(position) else {
            return Vec::new();
        };

        let inside_interpolation_expression = is_inside_interpolation_expression(&line_prefix);

        if self.is_inside_multiline_string_literal(position) && !inside_interpolation_expression {
            return Vec::new();
        }

        let completion_scope = self.completion_scope(position);

        if completion_scope == CompletionScope::TypedDeclarations {
            if !line_prefix.contains(':') {
                return Vec::new();
            }

            let semantic_index = self.semantic_index_for_completion(position);
            let current_schema_name = semantic_index.schema_name_at_position(position);

            return semantic_index.type_suggestions(&line_prefix, current_schema_name);
        }

        let semantic_index = self.semantic_index_for_completion(position);
        let line_has_property_separator = line_prefix.trim_start().contains(':');
        let should_include_builtin_function_suggestions = line_has_property_separator || inside_interpolation_expression;

        if !line_has_property_separator && !inside_interpolation_expression {
            match completion_scope {
                CompletionScope::InferenceSettings => {
                    return inference_setting_scope_suggestions(&line_prefix);
                }
                CompletionScope::AgentProperties => {
                    return agent_property_scope_suggestions(&line_prefix);
                }
                CompletionScope::General | CompletionScope::TypedDeclarations => {}
            }
        }

        if let Some(model_call_context) = ModelCallCompletionContext::from_line_prefix(&line_prefix) {
            let model_suggestions = semantic_index.model_call_suggestions(&model_call_context);

            if !model_suggestions.is_empty() {
                return model_suggestions;
            }
        }

        if let Some(provider_driver_suggestions) = semantic_index.provider_driver_value_suggestions(position, &line_prefix) {
            return provider_driver_suggestions;
        }

        if let Some(provider_property_suggestions) = semantic_index.provider_property_suggestions(position, &line_prefix) {
            return provider_property_suggestions;
        }

        if let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(&line_prefix) {
            let reference_completion_constraint = ReferenceCompletionConstraint::from_line_prefix(&line_prefix);
            let reference_suggestions =
                semantic_index.reference_path_suggestions(&reference_completion_path, reference_completion_constraint, position);

            if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
                return reference_suggestions;
            }

            if reference_completion_path.root_keyword() == Some(ReferenceKeyword::Tool) {
                return reference_suggestions;
            }

            if !reference_suggestions.is_empty() {
                return reference_suggestions;
            }
        }

        if semantic_index.is_type_position(position, &line_prefix) {
            let current_schema_name = semantic_index.schema_name_at_position(position);
            let type_suggestions = semantic_index.type_suggestions(&line_prefix, current_schema_name);

            if !type_suggestions.is_empty() {
                return type_suggestions;
            }
        }

        semantic_index.default_suggestions(should_include_builtin_function_suggestions)
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

    fn semantic_index_for_completion(&self, position: Position) -> SemanticIndex {
        if self.semantic_snapshot.parse_error.is_none() {
            return self.semantic_snapshot.semantic_index.clone();
        }

        self.recovered_semantic_index(position)
            .unwrap_or_else(|| self.semantic_snapshot.semantic_index.clone())
    }

    fn recovered_semantic_index(&self, position: Position) -> Option<SemanticIndex> {
        let cursor_offset = byte_offset_for_position(&self.text, position)?;

        let mut recovered_source = String::with_capacity(self.text.len() + COMPLETION_RECOVERY_PLACEHOLDER.len());
        recovered_source.push_str(&self.text[..cursor_offset]);
        recovered_source.push_str(COMPLETION_RECOVERY_PLACEHOLDER);
        recovered_source.push_str(&self.text[cursor_offset..]);

        let workflow = parse_workflow(&recovered_source).ok()?;

        Some(SemanticIndex::from_workflow(&workflow))
    }

    fn completion_scope(&self, position: Position) -> CompletionScope {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return CompletionScope::General;
        };

        completion_scope_at_offset(&self.text, cursor_offset)
    }

    fn is_inside_multiline_string_literal(&self, position: Position) -> bool {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return false;
        };

        is_inside_multiline_string_literal(&self.text, cursor_offset)
    }
}

#[derive(Debug, Clone, Default)]
struct SemanticIndex {
    providers: HashMap<String, ProviderSummary>,
    provider_locations: Vec<NamedSpan>,
    schemas: HashMap<String, SchemaSummary>,
    schema_names: Vec<String>,
    schema_locations: Vec<NamedSpan>,
    input_fields: BTreeMap<String, TypeExpression>,
    secrets_fields: BTreeMap<String, TypeExpression>,
    agents: HashMap<String, AgentSummary>,
    agent_names: Vec<String>,
    output_locations: Vec<SourceSpan>,
    typed_declaration_locations: Vec<SourceSpan>,
    agent_locations: Vec<NamedSpan>,
}

impl SemanticIndex {
    fn from_workflow(workflow: &Workflow) -> Self {
        let mut semantic_index = Self::default();

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    semantic_index.insert_provider(provider_declaration);
                }
                Declaration::Schema(schema_declaration) => {
                    let schema_fields = schema_declaration
                        .fields
                        .iter()
                        .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
                        .collect::<BTreeMap<_, _>>();

                    semantic_index
                        .schemas
                        .insert(schema_declaration.name.clone(), SchemaSummary { fields: schema_fields });

                    semantic_index.schema_names.push(schema_declaration.name.clone());
                    semantic_index.schema_locations.push(NamedSpan {
                        name: schema_declaration.name.clone(),
                        span: schema_declaration.span,
                    });
                    semantic_index.typed_declaration_locations.push(schema_declaration.span);
                }
                Declaration::Input(input_declaration) => {
                    if semantic_index.input_fields.is_empty() {
                        semantic_index.input_fields = typed_fields_to_map(&input_declaration.fields);
                    }

                    semantic_index.typed_declaration_locations.push(input_declaration.span);
                }
                Declaration::Secrets(secrets_declaration) => {
                    if semantic_index.secrets_fields.is_empty() {
                        semantic_index.secrets_fields = typed_fields_to_map(&secrets_declaration.fields);
                    }

                    semantic_index.typed_declaration_locations.push(secrets_declaration.span);
                }
                Declaration::Agent(agent_declaration) => {
                    let output_type = agent_declaration.properties.iter().find_map(|agent_property| match agent_property {
                        AgentProperty::Output(type_expression) => Some(type_expression.clone()),
                        AgentProperty::Model(_)
                        | AgentProperty::Prompt(_)
                        | AgentProperty::Context(_)
                        | AgentProperty::Inference(_)
                        | AgentProperty::Tools(_)
                        | AgentProperty::Custom { name: _, value: _ } => None,
                    });

                    semantic_index
                        .agents
                        .insert(agent_declaration.name.clone(), AgentSummary { output_type });

                    semantic_index.agent_names.push(agent_declaration.name.clone());
                    semantic_index.agent_locations.push(NamedSpan {
                        name: agent_declaration.name.clone(),
                        span: agent_declaration.span,
                    });
                }
                Declaration::Output(output_declaration) => {
                    semantic_index.output_locations.push(output_declaration.span);
                }
            }
        }

        semantic_index.schema_names.sort();
        semantic_index.schema_names.dedup();

        semantic_index.agent_names.sort();
        semantic_index.agent_names.dedup();

        semantic_index
    }

    fn from_text_fallback(source_text: &str) -> Self {
        let provider_names = collect_named_declaration_names(source_text, DeclarationKeyword::Provider);
        let schema_names = collect_named_declaration_names(source_text, DeclarationKeyword::Schema);
        let agent_names = collect_named_declaration_names(source_text, DeclarationKeyword::Agent);

        let input_fields = collect_singleton_block_field_names(source_text, SingletonDeclarationKind::Input)
            .into_iter()
            .map(|field_name| (field_name, TypeExpression::String))
            .collect::<BTreeMap<_, _>>();

        let secrets_fields = collect_singleton_block_field_names(source_text, SingletonDeclarationKind::Secrets)
            .into_iter()
            .map(|field_name| (field_name, TypeExpression::String))
            .collect::<BTreeMap<_, _>>();

        Self {
            providers: provider_names
                .iter()
                .map(|provider_name| {
                    (
                        provider_name.clone(),
                        ProviderSummary {
                            driver: None,
                            models: Vec::new(),
                        },
                    )
                })
                .collect(),
            provider_locations: Vec::new(),
            schemas: schema_names
                .iter()
                .map(|schema_name| (schema_name.clone(), SchemaSummary { fields: BTreeMap::new() }))
                .collect(),
            schema_names,
            schema_locations: Vec::new(),
            input_fields,
            secrets_fields,
            agents: agent_names
                .iter()
                .map(|agent_name| (agent_name.clone(), AgentSummary { output_type: None }))
                .collect(),
            agent_names,
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_locations: Vec::new(),
        }
    }

    fn insert_provider(&mut self, provider_declaration: &ProviderDeclaration) {
        let provider_driver = provider_declaration
            .properties
            .iter()
            .find(|provider_property| provider_property.name == "driver")
            .and_then(|provider_property| match &provider_property.value {
                Expression::StringLiteral(driver_name) => ProviderDriver::parse(driver_name),
                Expression::StringTemplate(_)
                | Expression::NumberLiteral(_)
                | Expression::BooleanLiteral(_)
                | Expression::NullLiteral
                | Expression::Reference(_)
                | Expression::FunctionCall(_)
                | Expression::ArrayLiteral(_)
                | Expression::ObjectLiteral(_) => None,
            });

        let provider_models = provider_declaration
            .properties
            .iter()
            .find(|provider_property| provider_property.name == "models")
            .and_then(|provider_property| extract_models(&provider_property.value))
            .unwrap_or_default();

        self.providers.insert(
            provider_declaration.name.clone(),
            ProviderSummary {
                driver: provider_driver,
                models: provider_models,
            },
        );

        self.provider_locations.push(NamedSpan {
            name: provider_declaration.name.clone(),
            span: provider_declaration.span,
        });
    }

    fn model_call_suggestions(&self, model_call_context: &ModelCallCompletionContext) -> Vec<CompletionSuggestion> {
        let Some(provider_summary) = self.providers.get(&model_call_context.provider_name) else {
            return Vec::new();
        };

        let mut completion_suggestions = provider_summary
            .models
            .iter()
            .filter(|model_name| model_name.starts_with(&model_call_context.model_prefix))
            .map(|model_name| {
                let insert_text = if model_call_context.inside_string_literal {
                    model_name.clone()
                } else {
                    format!("\"{model_name}\"")
                };

                CompletionSuggestion {
                    label: model_name.clone(),
                    kind: CompletionKind::Value,
                    detail: format!("Model from `{}` provider", model_call_context.provider_name),
                    documentation: "Model declared in provider `models` list.".to_string(),
                    insert_text,
                }
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    fn provider_driver_value_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let provider_name = self.provider_name_at_position(position)?;
        let _ = provider_name;

        let trimmed_line_prefix = line_prefix.trim_start();
        let (property_name, property_value_prefix) = trimmed_line_prefix.split_once(':')?;

        if property_name.trim() != "driver" {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(property_value_prefix);
        let mut completion_suggestions = ProviderDriver::all()
            .into_iter()
            .map(engine_ai_core::ProviderDriver::as_str)
            .filter(|driver_name| driver_name.starts_with(&value_completion_context.value_prefix))
            .map(|driver_name| {
                let insert_text = if value_completion_context.inside_string_literal {
                    driver_name.to_string()
                } else {
                    format!("\"{driver_name}\"")
                };

                CompletionSuggestion {
                    label: driver_name.to_string(),
                    kind: CompletionKind::Value,
                    detail: "Provider driver".to_string(),
                    documentation: "Valid provider driver value.".to_string(),
                    insert_text,
                }
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        Some(completion_suggestions)
    }

    fn provider_property_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let provider_name = self.provider_name_at_position(position)?;

        if line_prefix.trim_start().contains(':') {
            return None;
        }

        let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();
        let mut provider_property_names = self.provider_property_names(provider_name);

        provider_property_names.retain(|property_name| property_name.starts_with(property_prefix));

        if provider_property_names.is_empty() {
            return None;
        }

        let completion_suggestions = provider_property_names
            .into_iter()
            .map(|property_name| CompletionSuggestion {
                label: property_name.to_string(),
                kind: CompletionKind::Property,
                detail: format!("`{provider_name}` provider property"),
                documentation: "Provider configuration property.".to_string(),
                insert_text: property_name.to_string(),
            })
            .collect::<Vec<_>>();

        Some(completion_suggestions)
    }

    fn provider_property_names(&self, provider_name: &str) -> Vec<&'static str> {
        let Some(provider_summary) = self.providers.get(provider_name) else {
            return all_provider_property_names();
        };

        if let Some(provider_driver) = provider_summary.driver {
            return provider_driver.available_property_names().to_vec();
        }

        all_provider_property_names()
    }

    fn reference_path_suggestions(
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

    fn resolve_access_path(&self, start_types: Vec<TypeExpression>, accesses: &[String]) -> Vec<TypeExpression> {
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

    fn is_type_position(&self, position: Position, line_prefix: &str) -> bool {
        let trimmed_line_prefix = line_prefix.trim_end();

        if trimmed_line_prefix.ends_with("schema.") {
            return true;
        }

        let looks_like_type_trigger = trimmed_line_prefix.ends_with(':')
            || trimmed_line_prefix.ends_with('|')
            || trimmed_line_prefix.ends_with('[')
            || trimmed_line_prefix.ends_with('(')
            || trimmed_line_prefix.ends_with(',');

        if !looks_like_type_trigger {
            return false;
        }

        let inside_output = self
            .output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position));

        if inside_output {
            return false;
        }

        let inside_typed_declaration = self
            .typed_declaration_locations
            .iter()
            .copied()
            .any(|typed_declaration_span| source_span_contains_position(typed_declaration_span, position));

        if inside_typed_declaration {
            return true;
        }

        let inside_agent = self
            .agent_locations
            .iter()
            .any(|agent_location| source_span_contains_position(agent_location.span, position));

        if !inside_agent {
            return false;
        }

        trimmed_line_prefix.contains("output:")
    }

    fn type_suggestions(&self, line_prefix: &str, current_schema_name: Option<&str>) -> Vec<CompletionSuggestion> {
        if line_prefix.trim_end().ends_with("schema.") {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionKind::Type,
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
                    kind: CompletionKind::Type,
                    detail: "Named schema reference".to_string(),
                    documentation: "Reference a named schema type.".to_string(),
                    insert_text: format!("schema.{schema_name}"),
                }),
        );

        completion_suggestions
    }

    fn default_suggestions(&self, include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = builtin_symbol_suggestions(include_builtin_function_suggestions);

        completion_suggestions.extend(self.providers.keys().map(|provider_name| CompletionSuggestion {
            label: provider_name.clone(),
            kind: CompletionKind::Function,
            detail: "Declared provider".to_string(),
            documentation: "Provider call used in `model` properties.".to_string(),
            insert_text: provider_name.clone(),
        }));

        completion_suggestions.extend(self.agent_names.iter().map(|agent_name| CompletionSuggestion {
            label: agent_name.clone(),
            kind: CompletionKind::Variable,
            detail: "Declared agent".to_string(),
            documentation: "Agent declared in this document.".to_string(),
            insert_text: agent_name.clone(),
        }));

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    fn hover_markdown(&self, hovered_symbol: &str) -> Option<String> {
        if let Some(provider_summary) = self.providers.get(hovered_symbol) {
            let provider_driver_name = provider_summary.driver.map_or("unknown", ProviderDriver::as_str);

            return Some(format!(
                "**provider {hovered_symbol}**\n\nDriver: `{provider_driver_name}`\n\nDeclared models: {}",
                if provider_summary.models.is_empty() {
                    "none".to_string()
                } else {
                    provider_summary.models.join(", ")
                }
            ));
        }

        let reference_completion_path = ReferenceCompletionPath::from_token(hovered_symbol)?;
        let mut resolved_accesses = reference_completion_path.complete_accesses.clone();

        if !reference_completion_path.pending_prefix.is_empty() {
            resolved_accesses.push(reference_completion_path.pending_prefix.clone());
        }

        if reference_completion_path.is_schema_root() {
            let schema_name = resolved_accesses.first()?;
            let schema_summary = self.schemas.get(schema_name)?;

            return Some(format!(
                "**schema.{schema_name}**\n\nFields: {}",
                schema_summary
                    .fields
                    .iter()
                    .map(|(field_name, field_type)| format!("`{field_name}: {}`", field_type.render_type()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input) => {
                let field_type = resolve_singleton_reference_type(&self.input_fields, resolved_accesses.as_slice(), self)?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Secrets) => {
                let field_type = resolve_singleton_reference_type(&self.secrets_fields, resolved_accesses.as_slice(), self)?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Agent) => {
                let agent_name = resolved_accesses.first()?;
                let agent_summary = self.agents.get(agent_name)?;

                let agent_output_type = agent_summary.output_type.as_ref()?;

                if resolved_accesses.len() == 1 {
                    return Some(format!(
                        "**agent.{agent_name}**\n\nOutput type: `{}`",
                        agent_output_type.render_type()
                    ));
                }

                let candidate_types = self.resolve_access_path(vec![agent_output_type.clone()], &resolved_accesses[1..]);
                let final_type = candidate_types.first()?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, final_type.render_type()))
            }
            Some(ReferenceKeyword::Tool) | None => None,
        }
    }

    fn provider_name_at_position(&self, position: Position) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| source_span_contains_position(provider_location.span, position))
            .map(|provider_location| provider_location.name.as_str())
    }

    fn schema_name_at_position(&self, position: Position) -> Option<&str> {
        self.schema_locations
            .iter()
            .find(|schema_location| source_span_contains_position(schema_location.span, position))
            .map(|schema_location| schema_location.name.as_str())
    }

    fn agent_name_at_position(&self, position: Position) -> Option<&str> {
        self.agent_locations
            .iter()
            .find(|agent_location| source_span_contains_position(agent_location.span, position))
            .map(|agent_location| agent_location.name.as_str())
    }
}

fn resolve_singleton_reference_type(
    root_fields: &BTreeMap<String, TypeExpression>,
    resolved_accesses: &[String],
    semantic_index: &SemanticIndex,
) -> Option<TypeExpression> {
    let first_field_name = resolved_accesses.first()?;
    let root_field_type = root_fields.get(first_field_name)?.clone();

    if resolved_accesses.len() == 1 {
        return Some(root_field_type);
    }

    let candidate_types = semantic_index.resolve_access_path(vec![root_field_type], &resolved_accesses[1..]);

    candidate_types.first().cloned()
}

#[derive(Debug, Clone)]
struct ProviderSummary {
    driver: Option<ProviderDriver>,
    models: Vec<String>,
}

#[derive(Debug, Clone)]
struct SchemaSummary {
    fields: BTreeMap<String, TypeExpression>,
}

#[derive(Debug, Clone)]
struct AgentSummary {
    output_type: Option<TypeExpression>,
}

#[derive(Debug, Clone)]
struct NamedSpan {
    name: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ModelCallCompletionContext {
    provider_name: String,
    model_prefix: String,
    inside_string_literal: bool,
}

impl ModelCallCompletionContext {
    fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let trimmed_prefix = line_prefix.trim_end();
        let open_parenthesis_index = trimmed_prefix.rfind('(')?;

        let callee_prefix = trimmed_prefix[..open_parenthesis_index].trim_end();
        let provider_name = trailing_identifier(callee_prefix)?.to_string();
        let argument_prefix = &trimmed_prefix[open_parenthesis_index + 1..];

        if argument_prefix.contains(')') {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(argument_prefix);

        Some(Self {
            provider_name,
            model_prefix: value_completion_context.value_prefix,
            inside_string_literal: value_completion_context.inside_string_literal,
        })
    }
}

#[derive(Debug, Clone)]
struct ValueCompletionContext {
    value_prefix: String,
    inside_string_literal: bool,
}

impl ValueCompletionContext {
    fn from_value_prefix(value_prefix: &str) -> Self {
        let trimmed_value_prefix = value_prefix.trim_start();
        let quotation_count = trimmed_value_prefix.chars().filter(|character| *character == '"').count();

        if quotation_count % 2 == 1 {
            let last_quote_index = trimmed_value_prefix.rfind('"').unwrap_or(0);

            return Self {
                value_prefix: trimmed_value_prefix[last_quote_index + 1..].to_string(),
                inside_string_literal: true,
            };
        }

        Self {
            value_prefix: trimmed_value_prefix.to_string(),
            inside_string_literal: false,
        }
    }
}

#[derive(Debug, Clone)]
struct ReferenceCompletionPath {
    root: String,
    complete_accesses: Vec<String>,
    pending_prefix: String,
}

impl ReferenceCompletionPath {
    fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let reference_token = trailing_reference_token(line_prefix)?;

        Self::from_token(reference_token)
    }

    fn from_token(reference_token: &str) -> Option<Self> {
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

    fn root_keyword(&self) -> Option<ReferenceKeyword> {
        ReferenceKeyword::from_identifier(&self.root)
    }

    fn root_declaration_keyword(&self) -> Option<DeclarationKeyword> {
        DeclarationKeyword::from_identifier(&self.root)
    }

    fn is_schema_root(&self) -> bool {
        self.root_declaration_keyword() == Some(DeclarationKeyword::Schema)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceCompletionConstraint {
    None,
    ForLoopIterable,
}

impl ReferenceCompletionConstraint {
    fn from_line_prefix(line_prefix: &str) -> Self {
        if is_for_loop_iterable_reference_context(line_prefix) {
            return Self::ForLoopIterable;
        }

        Self::None
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

fn extract_models(model_expression: &Expression) -> Option<Vec<String>> {
    let Expression::ArrayLiteral(model_values) = model_expression else {
        return None;
    };

    let mut model_names = Vec::<String>::new();

    for model_value in model_values {
        let Expression::StringLiteral(model_name) = model_value else {
            return None;
        };

        model_names.push(model_name.clone());
    }

    Some(model_names)
}

fn typed_fields_to_map(typed_fields: &[TypedField]) -> BTreeMap<String, TypeExpression> {
    typed_fields
        .iter()
        .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
        .collect()
}

fn collect_named_declaration_names(source_text: &str, declaration_keyword: DeclarationKeyword) -> Vec<String> {
    let declaration_keyword = declaration_keyword.as_str();
    let mut names = HashSet::<String>::new();

    for source_line in source_text.lines() {
        let trimmed_line = source_line.trim_start();

        let Some(line_after_keyword) = trimmed_line.strip_prefix(declaration_keyword) else {
            continue;
        };

        if !line_after_keyword.starts_with(char::is_whitespace) {
            continue;
        }

        let declaration_name = line_after_keyword
            .trim_start()
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect::<String>();

        if declaration_name.is_empty() {
            continue;
        }

        names.insert(declaration_name);
    }

    let mut sorted_names = names.into_iter().collect::<Vec<_>>();
    sorted_names.sort();

    sorted_names
}

fn collect_singleton_block_field_names(source_text: &str, singleton_declaration_kind: SingletonDeclarationKind) -> Vec<String> {
    let block_name = singleton_declaration_kind.as_str();
    let mut field_names = HashSet::<String>::new();
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
            if let Some(field_name) = line_field_name(trimmed_line) {
                field_names.insert(field_name);
            }
        }

        let open_brace_count = isize::try_from(trimmed_line.chars().filter(|character| *character == '{').count()).unwrap_or(isize::MAX);
        let close_brace_count = isize::try_from(trimmed_line.chars().filter(|character| *character == '}').count()).unwrap_or(isize::MAX);

        brace_depth = brace_depth.saturating_add(open_brace_count);
        brace_depth = brace_depth.saturating_sub(close_brace_count);

        if brace_depth <= 0 {
            inside_block = false;
            brace_depth = 0;
        }
    }

    let mut sorted_field_names = field_names.into_iter().collect::<Vec<_>>();
    sorted_field_names.sort();

    sorted_field_names
}

fn line_field_name(source_line: &str) -> Option<String> {
    let (field_name, _) = source_line.split_once(':')?;
    let field_name = field_name.trim();

    if !is_identifier(field_name) {
        return None;
    }

    Some(field_name.to_string())
}

fn trailing_identifier(line_prefix: &str) -> Option<&str> {
    let mut start_index = line_prefix.len();

    for (character_index, character) in line_prefix.char_indices().rev() {
        if character.is_ascii_alphanumeric() || character == '_' {
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

fn is_inside_interpolation_expression(line_prefix: &str) -> bool {
    let open_count = line_prefix.match_indices("{{").count();
    let close_count = line_prefix.match_indices("}}").count();

    open_count > close_count
}

fn is_inside_multiline_string_literal(source_text: &str, cursor_offset: usize) -> bool {
    let source_prefix = &source_text[..cursor_offset];
    let triple_quote_count = source_prefix.match_indices("\"\"\"").count();

    triple_quote_count % 2 == 1
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

fn byte_offset_for_position(source_text: &str, position: Position) -> Option<usize> {
    let target_line = position.line as usize;
    let target_character = position.character as usize;

    let mut current_line = 0_usize;
    let mut current_character = 0_usize;

    for (byte_offset, character) in source_text.char_indices() {
        if current_line == target_line && current_character == target_character {
            return Some(byte_offset);
        }

        if character == '\n' {
            if current_line == target_line {
                return Some(byte_offset);
            }

            current_line += 1;
            current_character = 0;
            continue;
        }

        if current_line == target_line {
            current_character += 1;
        }
    }

    if current_line == target_line {
        return Some(source_text.len());
    }

    None
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
