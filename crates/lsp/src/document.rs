use std::collections::{BTreeMap, HashMap, HashSet};

use engine_ai_core::dsl::{
    parse_workflow, validate_workflow, AgentProperty, Declaration, DslParseError, Expression, ProviderDeclaration, SourcePosition,
    SourceSpan, TypeExpression, TypedField, ValidationIssue, ValidationReport, Workflow,
};
use engine_ai_core::runtime::{InferenceSetting, ProviderDriver};

use crate::protocol::{DiagnosticCode, Position, Range};

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

        let completion_scope = self.completion_scope(position);

        if completion_scope == CompletionScope::TypedDeclarations {
            if !line_prefix.contains(':') {
                return Vec::new();
            }

            let semantic_index = self.semantic_index_for_completion(position);

            return semantic_index.type_suggestions(&line_prefix);
        }

        let semantic_index = self.semantic_index_for_completion(position);
        let line_has_property_separator = line_prefix.trim_start().contains(':');

        if !line_has_property_separator {
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
            let reference_suggestions = semantic_index.reference_path_suggestions(&reference_completion_path);

            if !reference_suggestions.is_empty() {
                return reference_suggestions;
            }
        }

        if semantic_index.is_type_position(position, &line_prefix) {
            let type_suggestions = semantic_index.type_suggestions(&line_prefix);

            if !type_suggestions.is_empty() {
                return type_suggestions;
            }
        }

        semantic_index.default_suggestions()
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
}

#[derive(Debug)]
struct SemanticSnapshot {
    parse_error: Option<DslParseError>,
    validation_report: Option<ValidationReport>,
    semantic_index: SemanticIndex,
}

impl SemanticSnapshot {
    fn from_text(source_text: &str) -> Self {
        match parse_workflow(source_text) {
            Ok(workflow) => {
                let validation_report = validate_workflow(&workflow);
                let semantic_index = SemanticIndex::from_workflow(&workflow);

                Self {
                    parse_error: None,
                    validation_report: Some(validation_report),
                    semantic_index,
                }
            }
            Err(parse_error) => Self {
                parse_error: Some(parse_error),
                validation_report: None,
                semantic_index: SemanticIndex::from_text_fallback(source_text),
            },
        }
    }

    fn diagnostics(&self, source_text: &str) -> Vec<DocumentDiagnostic> {
        if self.parse_error.is_some() {
            return self.parse_diagnostics(source_text);
        }

        let Some(validation_report) = &self.validation_report else {
            return Vec::new();
        };

        validation_report
            .issues_with_spans()
            .map(|(validation_issue, optional_span)| {
                let range = optional_span.map_or_else(zero_range, |source_span| source_span_to_range(source_text, source_span));

                DocumentDiagnostic {
                    range,
                    severity: DiagnosticSeverity::Error,
                    code: DiagnosticCode::from(validation_issue),
                    message: validation_issue.message(),
                }
            })
            .collect()
    }

    fn parse_diagnostics(&self, source_text: &str) -> Vec<DocumentDiagnostic> {
        let Some(parse_error) = &self.parse_error else {
            return Vec::new();
        };

        let range = parse_error
            .span()
            .map_or_else(zero_range, |source_span| source_span_to_range(source_text, source_span));

        vec![DocumentDiagnostic {
            range,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::from(parse_error),
            message: parse_error.to_string(),
        }]
    }
}

#[derive(Debug, Clone)]
pub struct DocumentDiagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl DiagnosticSeverity {
    #[must_use]
    pub fn as_lsp_severity(self) -> u32 {
        match self {
            Self::Error => 1,
            Self::Warning => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionSuggestion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub documentation: String,
    pub insert_text: String,
}

#[derive(Debug, Clone, Copy)]
pub enum CompletionKind {
    Keyword,
    Function,
    Module,
    Property,
    Variable,
    Type,
    Value,
}

impl CompletionKind {
    #[must_use]
    pub fn as_lsp_kind(self) -> u32 {
        match self {
            Self::Keyword => 14,
            Self::Function => 3,
            Self::Module => 9,
            Self::Property => 10,
            Self::Variable => 6,
            Self::Type => 13,
            Self::Value => 12,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SemanticIndex {
    providers: HashMap<String, ProviderSummary>,
    provider_locations: Vec<NamedSpan>,
    schemas: HashMap<String, SchemaSummary>,
    schema_names: Vec<String>,
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
        let provider_names = collect_named_declaration_names(source_text, "provider");
        let schema_names = collect_named_declaration_names(source_text, "schema");
        let agent_names = collect_named_declaration_names(source_text, "agent");

        let input_fields = collect_singleton_block_field_names(source_text, "input")
            .into_iter()
            .map(|field_name| (field_name, TypeExpression::String))
            .collect::<BTreeMap<_, _>>();

        let secrets_fields = collect_singleton_block_field_names(source_text, "secrets")
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

    fn reference_path_suggestions(&self, reference_completion_path: &ReferenceCompletionPath) -> Vec<CompletionSuggestion> {
        match reference_completion_path.root.as_str() {
            "input" => self.singleton_reference_suggestions(
                &self.input_fields,
                &reference_completion_path.complete_accesses,
                &reference_completion_path.pending_prefix,
                "Input field",
            ),
            "secrets" => self.singleton_reference_suggestions(
                &self.secrets_fields,
                &reference_completion_path.complete_accesses,
                &reference_completion_path.pending_prefix,
                "Secrets field",
            ),
            "agent" => self.agent_reference_suggestions(reference_completion_path),
            "schema" => self.schema_reference_suggestions(reference_completion_path),
            "tool" => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn singleton_reference_suggestions(
        &self,
        root_fields: &BTreeMap<String, TypeExpression>,
        complete_accesses: &[String],
        pending_prefix: &str,
        detail_prefix: &str,
    ) -> Vec<CompletionSuggestion> {
        if complete_accesses.is_empty() {
            return root_fields
                .iter()
                .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
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

        self.field_suggestions_from_types(candidate_types.as_slice(), pending_prefix)
    }

    fn agent_reference_suggestions(&self, reference_completion_path: &ReferenceCompletionPath) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .agent_names
                .iter()
                .filter(|agent_name| agent_name.starts_with(&reference_completion_path.pending_prefix))
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
        let Some(agent_summary) = self.agents.get(agent_name) else {
            return Vec::new();
        };

        let Some(agent_output_type) = agent_summary.output_type.clone() else {
            return Vec::new();
        };

        let remaining_accesses = &reference_completion_path.complete_accesses[1..];
        let candidate_types = self.resolve_access_path(vec![agent_output_type], remaining_accesses);

        self.field_suggestions_from_types(candidate_types.as_slice(), &reference_completion_path.pending_prefix)
    }

    fn schema_reference_suggestions(&self, reference_completion_path: &ReferenceCompletionPath) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .schema_names
                .iter()
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

        self.field_suggestions_from_types(candidate_types.as_slice(), &reference_completion_path.pending_prefix)
    }

    fn field_suggestions_from_types(&self, candidate_types: &[TypeExpression], pending_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut available_fields = BTreeMap::<String, TypeExpression>::new();

        for candidate_type in candidate_types {
            self.collect_available_fields(candidate_type, &mut available_fields);
        }

        available_fields
            .into_iter()
            .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
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

    fn type_suggestions(&self, line_prefix: &str) -> Vec<CompletionSuggestion> {
        if line_prefix.trim_end().ends_with("schema.") {
            return self
                .schema_names
                .iter()
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

    fn default_suggestions(&self) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = builtin_symbol_suggestions();

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
            resolved_accesses.push(reference_completion_path.pending_prefix);
        }

        match reference_completion_path.root.as_str() {
            "input" => {
                let field_type = resolve_singleton_reference_type(&self.input_fields, resolved_accesses.as_slice(), self)?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            "secrets" => {
                let field_type = resolve_singleton_reference_type(&self.secrets_fields, resolved_accesses.as_slice(), self)?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            "agent" => {
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
            "schema" => {
                let schema_name = resolved_accesses.first()?;
                let schema_summary = self.schemas.get(schema_name)?;

                Some(format!(
                    "**schema.{schema_name}**\n\nFields: {}",
                    schema_summary
                        .fields
                        .iter()
                        .map(|(field_name, field_type)| format!("`{field_name}: {}`", field_type.render_type()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
            "tool" => None,
            _ => None,
        }
    }

    fn provider_name_at_position(&self, position: Position) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| source_span_contains_position(provider_location.span, position))
            .map(|provider_location| provider_location.name.as_str())
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
        let root = token_parts.first()?.to_string();

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

        let pending_prefix = token_parts.last()?.to_string();

        if !pending_prefix.is_empty() && !is_identifier(&pending_prefix) {
            return None;
        }

        Some(Self {
            root,
            complete_accesses,
            pending_prefix,
        })
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

fn collect_named_declaration_names(source_text: &str, declaration_keyword: &str) -> Vec<String> {
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

fn collect_singleton_block_field_names(source_text: &str, block_name: &str) -> Vec<String> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionScope {
    General,
    AgentProperties,
    InferenceSettings,
    TypedDeclarations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeBlock {
    Other,
    Agent,
    Inference,
    TypedDeclaration,
}

fn completion_scope_at_offset(source_text: &str, cursor_offset: usize) -> CompletionScope {
    let mut scope_blocks = Vec::<ScopeBlock>::new();
    let mut token_state = ScopeScannerTokenState::default();
    let mut string_state = ScopeScannerStringState::default();

    for character in source_text[..cursor_offset].chars() {
        if string_state.accept(character) {
            continue;
        }

        if character.is_ascii_alphanumeric() || character == '_' {
            token_state.current_identifier.push(character);
            continue;
        }

        token_state.flush_identifier();

        match character {
            ':' => {
                token_state.pending_property = token_state.recent_identifiers.last().cloned();
            }
            '{' => {
                let block_kind = token_state.block_for_open_brace(scope_blocks.last().copied());
                scope_blocks.push(block_kind);
                token_state.clear_after_brace();
            }
            '}' => {
                let _ = scope_blocks.pop();
                token_state.clear_after_brace();
            }
            '\n' | '\r' | ';' => {
                token_state.clear_after_statement();
            }
            ',' => {
                token_state.pending_property = None;
            }
            _ => {}
        }
    }

    match scope_blocks.last().copied() {
        Some(ScopeBlock::Inference) => CompletionScope::InferenceSettings,
        Some(ScopeBlock::Agent) => CompletionScope::AgentProperties,
        Some(ScopeBlock::TypedDeclaration) => CompletionScope::TypedDeclarations,
        Some(ScopeBlock::Other) | None => CompletionScope::General,
    }
}

#[derive(Debug, Default)]
struct ScopeScannerTokenState {
    current_identifier: String,
    recent_identifiers: Vec<String>,
    pending_property: Option<String>,
}

impl ScopeScannerTokenState {
    fn flush_identifier(&mut self) {
        if self.current_identifier.is_empty() {
            return;
        }

        self.recent_identifiers.push(self.current_identifier.clone());
        self.current_identifier.clear();

        if self.recent_identifiers.len() > 6 {
            let _ = self.recent_identifiers.remove(0);
        }
    }

    fn block_for_open_brace(&self, parent_block: Option<ScopeBlock>) -> ScopeBlock {
        let Some(last_identifier) = self.recent_identifiers.last() else {
            return ScopeBlock::Other;
        };

        if parent_block == Some(ScopeBlock::TypedDeclaration) {
            return ScopeBlock::TypedDeclaration;
        }

        if let Some(pending_property) = &self.pending_property {
            if pending_property == "inference" && parent_block == Some(ScopeBlock::Agent) {
                return ScopeBlock::Inference;
            }
        }

        if last_identifier == "input" || last_identifier == "secrets" {
            return ScopeBlock::TypedDeclaration;
        }

        if self.recent_identifiers.len() >= 2 {
            let penultimate_identifier = &self.recent_identifiers[self.recent_identifiers.len() - 2];

            if penultimate_identifier == "agent" && last_identifier != "agent" {
                return ScopeBlock::Agent;
            }

            if penultimate_identifier == "schema" && last_identifier != "schema" {
                return ScopeBlock::TypedDeclaration;
            }
        }

        ScopeBlock::Other
    }

    fn clear_after_brace(&mut self) {
        self.pending_property = None;
        self.recent_identifiers.clear();
        self.current_identifier.clear();
    }

    fn clear_after_statement(&mut self) {
        self.pending_property = None;
        self.recent_identifiers.clear();
        self.current_identifier.clear();
    }
}

#[derive(Debug, Default)]
struct ScopeScannerStringState {
    inside_string: bool,
    escaping: bool,
}

impl ScopeScannerStringState {
    fn accept(&mut self, character: char) -> bool {
        if self.inside_string {
            if self.escaping {
                self.escaping = false;
                return true;
            }

            if character == '\\' {
                self.escaping = true;
                return true;
            }

            if character == '"' {
                self.inside_string = false;
            }

            return true;
        }

        if character == '"' {
            self.inside_string = true;
            return true;
        }

        false
    }
}

fn agent_property_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    AGENT_PROPERTY_DOCS
        .iter()
        .filter(|agent_property_doc| agent_property_doc.name.starts_with(property_prefix))
        .map(|agent_property_doc| CompletionSuggestion {
            label: agent_property_doc.name.to_string(),
            kind: CompletionKind::Property,
            detail: agent_property_doc.detail.to_string(),
            documentation: agent_property_doc.documentation.to_string(),
            insert_text: agent_property_doc.name.to_string(),
        })
        .collect()
}

fn inference_setting_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let setting_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    InferenceSetting::all()
        .into_iter()
        .filter(|inference_setting| inference_setting.key().starts_with(setting_prefix))
        .map(|inference_setting| CompletionSuggestion {
            label: inference_setting.key().to_string(),
            kind: CompletionKind::Property,
            detail: inference_setting.completion_detail().to_string(),
            documentation: inference_setting.completion_documentation().to_string(),
            insert_text: inference_setting.key().to_string(),
        })
        .collect()
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

impl From<&DslParseError> for DiagnosticCode {
    fn from(parse_error: &DslParseError) -> Self {
        match parse_error {
            DslParseError::Pest { message: _, span: _ } => Self::ParseError,
            DslParseError::MissingNode {
                expected: _,
                context: _,
                span: _,
            } => Self::MissingNode,
            DslParseError::UnexpectedRule {
                rule: _,
                context: _,
                span: _,
            } => Self::UnexpectedRule,
            DslParseError::InvalidIntegerLiteral {
                literal: _,
                context: _,
                span: _,
            } => Self::InvalidIntegerLiteral,
        }
    }
}

impl From<&ValidationIssue> for DiagnosticCode {
    fn from(validation_issue: &ValidationIssue) -> Self {
        match validation_issue {
            ValidationIssue::DuplicateProvider { provider_name: _ } => Self::DuplicateProvider,
            ValidationIssue::DuplicateSchema { schema_name: _ } => Self::DuplicateSchema,
            ValidationIssue::DuplicateAgent { agent_name: _ } => Self::DuplicateAgent,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind: _ } => Self::DuplicateSingletonDeclaration,
            ValidationIssue::UnknownAgentProperty {
                agent_name: _,
                property_name: _,
            } => Self::UnknownAgentProperty,
            ValidationIssue::InvalidModelExpression { agent_name: _ } => Self::InvalidModelExpression,
            ValidationIssue::UnknownProviderInModel {
                agent_name: _,
                provider_name: _,
            } => Self::UnknownProviderInModel,
            ValidationIssue::UnknownModelForProvider {
                agent_name: _,
                provider_name: _,
                model_name: _,
            } => Self::UnknownModelForProvider,
            ValidationIssue::UnknownAgentReference {
                referenced_agent: _,
                context: _,
            } => Self::UnknownAgentReference,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword: _, context: _ } => Self::InvalidKeywordReferenceRoot,
            ValidationIssue::MissingInputDeclaration { context: _ } => Self::MissingInputDeclaration,
            ValidationIssue::MissingSecretsDeclaration { context: _ } => Self::MissingSecretsDeclaration,
            ValidationIssue::UnknownInputFieldReference { field_name: _, context: _ } => Self::UnknownInputFieldReference,
            ValidationIssue::UnknownSecretsFieldReference { field_name: _, context: _ } => Self::UnknownSecretsFieldReference,
            ValidationIssue::SecretReferenceInLlmContext {
                reference_path: _,
                context: _,
            } => Self::SecretReferenceInLlmContext,
            ValidationIssue::MissingAgentOutputTypeForFieldReference { agent_name: _, context: _ } => {
                Self::MissingAgentOutputTypeForFieldReference
            }
            ValidationIssue::InvalidReferencePath {
                reference_path: _,
                invalid_field: _,
                context: _,
            } => Self::InvalidReferencePath,
            ValidationIssue::UnknownSchemaReference {
                referenced_schema: _,
                context: _,
            } => Self::UnknownSchemaReference,
            ValidationIssue::AgentDependencyCycle { agent_names: _ } => Self::AgentDependencyCycle,
        }
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

const BUILTIN_SYMBOL_DOCS: [BuiltinSymbolDoc; 14] = [
    BuiltinSymbolDoc {
        label: "provider",
        kind: CompletionKind::Keyword,
        detail: "Provider declaration",
        documentation: "Declares a provider configuration block.",
    },
    BuiltinSymbolDoc {
        label: "agent",
        kind: CompletionKind::Keyword,
        detail: "Agent declaration",
        documentation: "Declares an executable workflow agent.",
    },
    BuiltinSymbolDoc {
        label: "schema",
        kind: CompletionKind::Keyword,
        detail: "Schema declaration",
        documentation: "Declares a reusable named schema type.",
    },
    BuiltinSymbolDoc {
        label: "input",
        kind: CompletionKind::Keyword,
        detail: "Input declaration",
        documentation: "Declares workflow input fields.",
    },
    BuiltinSymbolDoc {
        label: "secrets",
        kind: CompletionKind::Keyword,
        detail: "Secrets declaration",
        documentation: "Declares workflow secret fields.",
    },
    BuiltinSymbolDoc {
        label: "output",
        kind: CompletionKind::Keyword,
        detail: "Output declaration",
        documentation: "Declares final workflow output fields.",
    },
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

#[derive(Debug, Clone, Copy)]
struct AgentPropertyDoc {
    name: &'static str,
    detail: &'static str,
    documentation: &'static str,
}

const AGENT_PROPERTY_DOCS: [AgentPropertyDoc; 5] = [
    AgentPropertyDoc {
        name: "model",
        detail: "Model binding (required)",
        documentation: "Selects provider and model call used by this agent.",
    },
    AgentPropertyDoc {
        name: "prompt",
        detail: "Prompt expression (required)",
        documentation: "Defines the prompt sent to the provider.",
    },
    AgentPropertyDoc {
        name: "output",
        detail: "Output type",
        documentation: "Declares the expected structured output type.",
    },
    AgentPropertyDoc {
        name: "inference",
        detail: "Inference settings object",
        documentation: "Configures sampling and provider retry behavior.",
    },
    AgentPropertyDoc {
        name: "context",
        detail: "Context expression",
        documentation: "Prepends evaluated context to the rendered prompt.",
    },
];

trait InferenceSettingCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl InferenceSettingCompletionDoc for InferenceSetting {
    fn completion_detail(self) -> &'static str {
        match self {
            Self::MaxTokens => "Token budget (integer)",
            Self::Temperature => "Sampling temperature (number)",
            Self::TopP => "Nucleus sampling top_p (number)",
            Self::TopK => "Top-k sampling limit (integer)",
            Self::FrequencyPenalty => "Frequency penalty (number)",
            Self::PresencePenalty => "Presence penalty (number)",
            Self::RepeatPenalty => "Repeat penalty (number)",
            Self::Seed => "Random seed (integer)",
            Self::StuckThreshold => "Stuck retry threshold (integer)",
            Self::ProviderMaxRetries => "Provider max retries (integer)",
            Self::ProviderRetryBaseDelayMs => "Retry base delay ms (integer)",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            Self::MaxTokens => "Maximum number of generated tokens.",
            Self::Temperature => "Controls randomness in token sampling.",
            Self::TopP => "Limits sampling to the smallest token set reaching cumulative probability `p`.",
            Self::TopK => "Limits sampling to the top `k` most likely tokens.",
            Self::FrequencyPenalty => "Penalizes tokens repeated frequently in generated output.",
            Self::PresencePenalty => "Penalizes tokens that already appeared in generated output.",
            Self::RepeatPenalty => "Applies multiplicative penalty to repeated tokens.",
            Self::Seed => "Sets deterministic random seed for repeatable generation.",
            Self::StuckThreshold => "Retry generation after this many stalled attempts.",
            Self::ProviderMaxRetries => "Maximum retries for provider-side failures.",
            Self::ProviderRetryBaseDelayMs => "Base backoff delay in milliseconds between provider retries.",
        }
    }
}

fn builtin_symbol_suggestions() -> Vec<CompletionSuggestion> {
    BUILTIN_SYMBOL_DOCS
        .iter()
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
    ["string", "number", "float", "boolean", "null"]
        .into_iter()
        .map(|type_name| CompletionSuggestion {
            label: type_name.to_string(),
            kind: CompletionKind::Type,
            detail: "Primitive type".to_string(),
            documentation: "Primitive workflow type.".to_string(),
            insert_text: type_name.to_string(),
        })
        .collect()
}

fn builtin_symbol_markdown(symbol_name: &str) -> Option<String> {
    let direct_match = BUILTIN_SYMBOL_DOCS
        .iter()
        .find(|builtin_symbol_doc| builtin_symbol_doc.label == symbol_name)
        .or_else(|| {
            symbol_name.rsplit('.').next().and_then(|symbol_suffix| {
                BUILTIN_SYMBOL_DOCS
                    .iter()
                    .find(|builtin_symbol_doc| builtin_symbol_doc.label == symbol_suffix)
            })
        })?;

    Some(format!(
        "**{}**\n\n{}\n\n{}",
        direct_match.label, direct_match.detail, direct_match.documentation
    ))
}

#[cfg(test)]
mod tests {
    use super::{DocumentState, Position};
    use crate::protocol::DiagnosticCode;

    macro_rules! inline_document_with_cursor {
        ($($workflow_tokens:tt)*) => {{
            source_with_cursor(stringify!($($workflow_tokens)*))
        }};
    }

    fn source_with_cursor(source_template: &str) -> (String, Position) {
        let normalized_template = normalize_inline_cursor_layout(source_template);
        let compact_cursor_marker = "<cursor>";
        let spaced_cursor_marker = "< cursor >";

        let (cursor_marker, cursor_byte_offset) = if let Some(marker_offset) = normalized_template.find(compact_cursor_marker) {
            (compact_cursor_marker, marker_offset)
        } else if let Some(marker_offset) = normalized_template.find(spaced_cursor_marker) {
            (spaced_cursor_marker, marker_offset)
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

    #[test]
    fn reports_parse_diagnostics_for_invalid_syntax() {
        let document_state = DocumentState::new("agent broken {\n    prompt: \"hello\"\n".to_string());
        let diagnostics = document_state.diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ParseError);
    }

    #[test]
    fn reports_unknown_model_for_provider_diagnostic() {
        let source = r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent writer {
                model: openai("gpt-4.1")
                prompt: "hello"
                output: string
            }
        "#;

        let document_state = DocumentState::new(source.to_string());
        let diagnostics = document_state.diagnostics();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownModelForProvider));
    }

    #[test]
    fn reports_unknown_agent_property_diagnostic() {
        let source = r#"
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
        "#;

        let document_state = DocumentState::new(source.to_string());
        let diagnostics = document_state.diagnostics();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownAgentProperty));
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

        dbg!(completion_suggestions
            .iter()
            .map(|completion_suggestion| completion_suggestion.label.clone())
            .collect::<Vec<_>>());

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "first"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "last"));
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

        dbg!(completion_suggestions
            .iter()
            .map(|completion_suggestion| completion_suggestion.label.clone())
            .collect::<Vec<_>>());

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "endpoint"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "api_key"));
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "model"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "prompt"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "output"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "provider"));
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "max_tokens"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "temperature"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "model"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "provider"));
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "prompt"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "max_tokens"));
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
            .find(|completion_suggestion| completion_suggestion.label == "max_tokens")
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "gpt-4.1-mini"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "gpt-4o-mini"));
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "Person"));
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

        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "string"));
        assert!(completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "number"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "provider"));
        assert!(!completion_suggestions
            .iter()
            .any(|completion_suggestion| completion_suggestion.label == "agent"));
    }
}
