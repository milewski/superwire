use std::collections::{BTreeMap, HashMap};

use engine_ai_core::dsl::{
    AgentProperty, BuiltinFunctionName, Declaration, DeclarationKeyword, Expression, ProviderDeclaration, ReferenceKeyword,
    SingletonDeclarationKind, SourceSpan, TypeExpression, TypedField, Workflow,
};
use engine_ai_core::runtime::ProviderDriver;
use engine_ai_core::semantic::{SemanticToolingSnapshot, ToolingSymbolCategory};

use crate::protocol::Position;

use super::completion_context::{ModelCallCompletionContext, ValueCompletionContext};
use super::hover::builtin_symbol_suggestions;
use super::position::source_span_contains_position;
use super::text_utils::trailing_identifier;
use super::{all_provider_property_names, type_symbol_suggestions, CompletionKind, CompletionSuggestion};

#[derive(Debug, Clone)]
pub(super) struct SemanticIndex {
    pub(in crate::document) providers: HashMap<String, ProviderSummary>,
    pub(in crate::document) provider_locations: Vec<NamedSpan>,
    pub(in crate::document) schemas: HashMap<String, SchemaSummary>,
    pub(in crate::document) schema_names: Vec<String>,
    pub(in crate::document) schema_locations: Vec<NamedSpan>,
    pub(in crate::document) input_fields: BTreeMap<String, TypeExpression>,
    pub(in crate::document) secrets_fields: BTreeMap<String, TypeExpression>,
    pub(in crate::document) agents: HashMap<String, AgentSummary>,
    pub(in crate::document) agent_names: Vec<String>,
    pub(in crate::document) output_locations: Vec<SourceSpan>,
    pub(in crate::document) typed_declaration_locations: Vec<SourceSpan>,
    pub(in crate::document) agent_locations: Vec<NamedSpan>,
    has_input_declaration: bool,
    has_secrets_declaration: bool,
    has_output_declaration: bool,
    pub(in crate::document) tooling_snapshot: SemanticToolingSnapshot,
}

#[derive(Debug, Clone)]
pub(in crate::document) struct ProviderSummary {
    pub(in crate::document) driver: Option<ProviderDriver>,
    pub(in crate::document) models: Vec<String>,
}

#[derive(Debug, Clone)]
pub(in crate::document) struct SchemaSummary {
    pub(in crate::document) fields: BTreeMap<String, TypeExpression>,
}

#[derive(Debug, Clone)]
pub(in crate::document) struct AgentSummary {
    pub(in crate::document) output_type: Option<TypeExpression>,
}

#[derive(Debug, Clone)]
pub(in crate::document) struct NamedSpan {
    pub(in crate::document) name: String,
    pub(in crate::document) span: SourceSpan,
}

impl SemanticIndex {
    pub(super) fn interpolation_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Interpolation reference root".to_string(),
                documentation: format!("Use `{}.<path>` inside interpolation expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub(super) fn context_function_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let context_function_label = BuiltinFunctionName::Context.as_str();

        if !context_function_label.starts_with(value_prefix) {
            return Vec::new();
        }

        builtin_symbol_suggestions(true)
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label == context_function_label)
            .collect()
    }

    pub(super) fn from_workflow(workflow: &Workflow) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(workflow);
        let mut semantic_index = Self {
            providers: HashMap::new(),
            provider_locations: Vec::new(),
            schemas: HashMap::new(),
            schema_names: Vec::new(),
            schema_locations: Vec::new(),
            input_fields: BTreeMap::new(),
            secrets_fields: BTreeMap::new(),
            agents: HashMap::new(),
            agent_names: Vec::new(),
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_locations: Vec::new(),
            has_input_declaration: false,
            has_secrets_declaration: false,
            has_output_declaration: false,
            tooling_snapshot,
        };

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
                    semantic_index.has_input_declaration = true;

                    if semantic_index.input_fields.is_empty() {
                        semantic_index.input_fields = typed_fields_to_map(&input_declaration.fields);
                    }

                    semantic_index.typed_declaration_locations.push(input_declaration.span);
                }
                Declaration::Secrets(secrets_declaration) => {
                    semantic_index.has_secrets_declaration = true;

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
                    semantic_index.has_output_declaration = true;
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

    pub(super) fn from_text_fallback(source_text: &str) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_source_tolerant(source_text);
        let mut semantic_index = Self::from_tooling_snapshot(&tooling_snapshot);

        semantic_index.has_input_declaration = semantic_index.has_input_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Input.as_str());
        semantic_index.has_secrets_declaration = semantic_index.has_secrets_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Secrets.as_str());
        semantic_index.has_output_declaration = semantic_index.has_output_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Output.as_str());

        semantic_index
    }

    fn from_tooling_snapshot(tooling_snapshot: &SemanticToolingSnapshot) -> Self {
        let providers = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| {
                (
                    named_symbol_span.name.clone(),
                    ProviderSummary {
                        driver: None,
                        models: Vec::new(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let provider_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        let schemas = tooling_snapshot
            .schemas()
            .iter()
            .map(|(schema_name, schema_fields)| {
                (
                    schema_name.clone(),
                    SchemaSummary {
                        fields: schema_fields.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut schema_names = tooling_snapshot.schemas().keys().cloned().collect::<Vec<_>>();
        schema_names.sort();
        schema_names.dedup();

        let schema_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Schema)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        let agents = tooling_snapshot
            .agents()
            .iter()
            .map(|(agent_name, agent_output_type)| {
                (
                    agent_name.clone(),
                    AgentSummary {
                        output_type: agent_output_type.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut agent_names = tooling_snapshot.agents().keys().cloned().collect::<Vec<_>>();
        agent_names.sort();
        agent_names.dedup();

        let agent_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Agent)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        Self {
            providers,
            provider_locations,
            schemas,
            schema_names,
            schema_locations,
            input_fields: tooling_snapshot.input_fields().clone(),
            secrets_fields: tooling_snapshot.secrets_fields().clone(),
            agents,
            agent_names,
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_locations,
            has_input_declaration: !tooling_snapshot.input_fields().is_empty(),
            has_secrets_declaration: !tooling_snapshot.secrets_fields().is_empty(),
            has_output_declaration: false,
            tooling_snapshot: tooling_snapshot.clone(),
        }
    }

    fn source_has_named_block_declaration(source_text: &str, declaration_keyword: &str) -> bool {
        for source_line in source_text.lines() {
            let trimmed_line = source_line.trim_start();
            let Some(line_after_keyword) = trimmed_line.strip_prefix(declaration_keyword) else {
                continue;
            };

            if !line_after_keyword.starts_with(char::is_whitespace) {
                continue;
            }

            if line_after_keyword.trim_start().starts_with('{') {
                return true;
            }
        }

        false
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

    pub(super) fn model_call_suggestions(&self, model_call_context: &ModelCallCompletionContext) -> Vec<CompletionSuggestion> {
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

    pub(super) fn provider_driver_value_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
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

    pub(super) fn provider_property_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
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

    pub(super) fn is_type_position(&self, position: Position, line_prefix: &str) -> bool {
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

    pub(super) fn type_suggestions(&self, line_prefix: &str, current_schema_name: Option<&str>) -> Vec<CompletionSuggestion> {
        let trimmed_line_prefix = line_prefix.trim_end();

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

    pub(super) fn root_declaration_suggestions(&self, line_prefix: &str) -> Vec<CompletionSuggestion> {
        let declaration_prefix = trailing_identifier(line_prefix).unwrap_or_default();

        builtin_symbol_suggestions(false)
            .into_iter()
            .filter(|completion_suggestion| matches!(completion_suggestion.kind, CompletionKind::Keyword))
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(declaration_prefix))
            .filter(|completion_suggestion| self.should_suggest_root_declaration_label(&completion_suggestion.label))
            .collect()
    }

    pub(super) fn is_output_position(&self, position: Position) -> bool {
        self.output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position))
    }

    fn should_suggest_root_declaration_label(&self, declaration_label: &str) -> bool {
        if declaration_label == DeclarationKeyword::Provider.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Agent.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Schema.as_str() {
            return true;
        }

        if declaration_label == SingletonDeclarationKind::Input.as_str() {
            return !self.has_input_declaration;
        }

        if declaration_label == SingletonDeclarationKind::Secrets.as_str() {
            return !self.has_secrets_declaration;
        }

        if declaration_label == SingletonDeclarationKind::Output.as_str() {
            return !self.has_output_declaration;
        }

        false
    }

    pub(super) fn default_suggestions(&self, include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
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

    pub(super) fn is_root_declaration_position(&self, position: Position) -> bool {
        if self
            .provider_locations
            .iter()
            .any(|provider_location| source_span_contains_position(provider_location.span, position))
        {
            return false;
        }

        if self
            .schema_locations
            .iter()
            .any(|schema_location| source_span_contains_position(schema_location.span, position))
        {
            return false;
        }

        if self
            .agent_locations
            .iter()
            .any(|agent_location| source_span_contains_position(agent_location.span, position))
        {
            return false;
        }

        if self
            .typed_declaration_locations
            .iter()
            .copied()
            .any(|typed_declaration_span| source_span_contains_position(typed_declaration_span, position))
        {
            return false;
        }

        if self
            .output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position))
        {
            return false;
        }

        true
    }

    pub(super) fn provider_name_at_position(&self, position: Position) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| source_span_contains_position(provider_location.span, position))
            .map(|provider_location| provider_location.name.as_str())
    }

    pub(super) fn schema_name_at_position(&self, position: Position) -> Option<&str> {
        self.schema_locations
            .iter()
            .find(|schema_location| source_span_contains_position(schema_location.span, position))
            .map(|schema_location| schema_location.name.as_str())
    }

    pub(super) fn agent_name_at_position(&self, position: Position) -> Option<&str> {
        self.agent_locations
            .iter()
            .find(|agent_location| source_span_contains_position(agent_location.span, position))
            .map(|agent_location| agent_location.name.as_str())
    }
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
