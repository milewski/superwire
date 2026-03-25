use std::collections::{BTreeMap, HashMap, HashSet};

use engine_ai_core::dsl::{
    AgentProperty, Declaration, DeclarationKeyword, Expression, ProviderDeclaration, SingletonDeclarationKind, SourceSpan, TypeExpression,
    TypedField, Workflow,
};
use engine_ai_core::runtime::ProviderDriver;

use crate::protocol::Position;

use super::completion_context::{ModelCallCompletionContext, ValueCompletionContext};
use super::hover::builtin_symbol_suggestions;
use super::position::source_span_contains_position;
use super::text_utils::{is_identifier, trailing_identifier};
use super::{all_provider_property_names, type_symbol_suggestions, CompletionKind, CompletionSuggestion};

#[derive(Debug, Clone, Default)]
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
    pub(super) fn from_workflow(workflow: &Workflow) -> Self {
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

    pub(super) fn from_text_fallback(source_text: &str) -> Self {
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
