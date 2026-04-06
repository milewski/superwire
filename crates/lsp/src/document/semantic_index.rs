use std::collections::{BTreeMap, HashMap};

use superwire_core::dsl::{
    AgentProperty, BuiltinFunctionName, Declaration, DeclarationKeyword, Expression, ProviderDeclaration, ReferenceKeyword,
    SingletonDeclarationKind, SourcePosition, SourceSpan, TypeExpression, TypedField, Workflow,
};
use superwire_core::runtime::ProviderDriver;
use superwire_core::semantic::{SemanticToolingSnapshot, ToolingSymbolCategory};

use crate::protocol::Position;

use super::completion_context::{ModelCallCompletionContext, ValueCompletionContext};
use super::hover::builtin_symbol_suggestions;
use super::position::source_span_contains_position;
use super::text_utils::trailing_identifier;
use super::{all_provider_property_names, type_symbol_suggestions, CompletionKind, CompletionSuggestion};

#[derive(Debug, Clone)]
pub struct SemanticIndex {
    pub providers: HashMap<String, ProviderSummary>,
    pub provider_locations: Vec<NamedSpan>,
    pub schemas: HashMap<String, SchemaSummary>,
    pub schema_names: Vec<String>,
    pub schema_locations: Vec<NamedSpan>,
    pub input_fields: BTreeMap<String, TypeExpression>,
    pub input_field_metadata: BTreeMap<String, FieldMetadata>,
    pub secrets_fields: BTreeMap<String, TypeExpression>,
    pub secrets_field_metadata: BTreeMap<String, FieldMetadata>,
    pub agents: HashMap<String, AgentSummary>,
    pub agent_for_loop_iterators: HashMap<String, String>,
    pub agent_for_loop_iterator_types: HashMap<String, TypeExpression>,
    pub agent_names: Vec<String>,
    pub output_locations: Vec<SourceSpan>,
    pub typed_declaration_locations: Vec<SourceSpan>,
    pub agent_locations: Vec<NamedSpan>,
    has_input_declaration: bool,
    has_secrets_declaration: bool,
    has_output_declaration: bool,
    pub tooling_snapshot: SemanticToolingSnapshot,
}

#[derive(Debug, Clone)]
pub struct ProviderSummary {
    pub driver: Option<ProviderDriver>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SchemaSummary {
    pub fields: BTreeMap<String, TypeExpression>,
    pub field_metadata: BTreeMap<String, FieldMetadata>,
}

#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub output_type: Option<TypeExpression>,
}

#[derive(Debug, Clone)]
pub struct FieldMetadata {
    pub field_type: TypeExpression,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NamedSpan {
    pub name: String,
    pub span: SourceSpan,
}

impl SemanticIndex {
    pub fn provider_call_suggestions(&self, provider_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self
            .providers
            .keys()
            .filter(|provider_name| provider_name.starts_with(provider_prefix))
            .map(|provider_name| CompletionSuggestion {
                label: provider_name.clone(),
                kind: CompletionKind::Function,
                detail: "Declared provider".to_string(),
                documentation: "Provider call used in `model` properties.".to_string(),
                insert_text: format!("{provider_name}(\"$1\")"),
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn model_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Input, ReferenceKeyword::Secrets]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Model expression reference root".to_string(),
                documentation: format!("Use `{}.<path>` inside model expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn inference_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Inference value reference root".to_string(),
                documentation: format!("Use `{}.<path>` for inference values.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn inference_object_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        if !"{}".starts_with(value_prefix) {
            return Vec::new();
        }

        vec![CompletionSuggestion {
            label: "{}".to_string(),
            kind: CompletionKind::Value,
            detail: "Inference settings object".to_string(),
            documentation: "Object literal for `inference` settings.".to_string(),
            insert_text: "{}".to_string(),
        }]
    }

    pub fn for_loop_iterable_value_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let root_prefix = trailing_identifier(value_prefix).unwrap_or_default();
        let mut completion_suggestions = [ReferenceKeyword::Agent, ReferenceKeyword::Input, ReferenceKeyword::Secrets]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "For-loop iterable reference root".to_string(),
                documentation: format!("Use `{}.<path>` in for-loop iterable expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect::<Vec<_>>();

        if value_prefix.trim().is_empty() {
            completion_suggestions.push(CompletionSuggestion {
                label: "[]".to_string(),
                kind: CompletionKind::Value,
                detail: "Array literal".to_string(),
                documentation: "Inline array literal iterable expression.".to_string(),
                insert_text: "[]".to_string(),
            });
        }

        completion_suggestions
    }

    pub fn output_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Input, ReferenceKeyword::Secrets]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Output value reference root".to_string(),
                documentation: format!("Use `{}.<path>` in output expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn output_value_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self.output_value_root_suggestions(value_prefix);
        let literal_suggestion_specs = [
            ("\"\"", "String literal"),
            ("0", "Number literal"),
            ("[]", "Array literal"),
            ("{}", "Object literal"),
            ("true", "Boolean literal"),
            ("false", "Boolean literal"),
            ("null", "Null literal"),
        ];

        completion_suggestions.extend(
            literal_suggestion_specs
                .into_iter()
                .filter(|(literal_label, _)| literal_label.starts_with(value_prefix))
                .map(|(literal_label, literal_detail)| CompletionSuggestion {
                    label: literal_label.to_string(),
                    kind: CompletionKind::Value,
                    detail: literal_detail.to_string(),
                    documentation: "Literal output value expression.".to_string(),
                    insert_text: literal_label.to_string(),
                }),
        );

        completion_suggestions
    }

    pub fn prompt_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Prompt value reference root".to_string(),
                documentation: format!("Use `{}.<path>` in prompt expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn prompt_value_suggestions(&self, value_prefix: &str, line_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self.prompt_value_root_suggestions(value_prefix);
        let single_line_literal = "\"\"";

        if single_line_literal.starts_with(value_prefix) {
            completion_suggestions.push(CompletionSuggestion {
                label: single_line_literal.to_string(),
                kind: CompletionKind::Value,
                detail: "String literal".to_string(),
                documentation: "Literal prompt expression.".to_string(),
                insert_text: single_line_literal.to_string(),
            });
        }

        let multiline_literal_label = "\"\"\"";
        let multiline_literal_indentation = Self::line_indentation(line_prefix);
        let multiline_literal_insert_text = format!("\"\"\"\n{multiline_literal_indentation}\"\"\"");

        if multiline_literal_label.starts_with(value_prefix) {
            completion_suggestions.push(CompletionSuggestion {
                label: multiline_literal_label.to_string(),
                kind: CompletionKind::Value,
                detail: "Multiline string literal".to_string(),
                documentation: "Literal prompt expression.".to_string(),
                insert_text: multiline_literal_insert_text,
            });
        }

        completion_suggestions
    }

    fn line_indentation(line_prefix: &str) -> &str {
        let indentation_length = line_prefix
            .char_indices()
            .find_map(|(character_offset, character)| (!character.is_whitespace()).then_some(character_offset))
            .unwrap_or(line_prefix.len());

        &line_prefix[..indentation_length]
    }

    pub fn interpolation_root_suggestions(&self, root_prefix: &str, position: Position) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = [ReferenceKeyword::Agent, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionKind::Module,
                detail: "Interpolation reference root".to_string(),
                documentation: format!("Use `{}.<path>` inside interpolation expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect::<Vec<_>>();

        if let Some(for_loop_iterator_name) = self.for_loop_iterator_name_at_position(position) {
            if for_loop_iterator_name.starts_with(root_prefix) {
                completion_suggestions.push(CompletionSuggestion {
                    label: for_loop_iterator_name.to_string(),
                    kind: CompletionKind::Variable,
                    detail: "For-loop iterator variable".to_string(),
                    documentation: "Iterator binding declared in the current agent for-clause.".to_string(),
                    insert_text: for_loop_iterator_name.to_string(),
                });
            }
        }

        completion_suggestions
    }

    pub fn context_function_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let context_function_label = BuiltinFunctionName::Context.as_str();

        if !context_function_label.starts_with(value_prefix) {
            return Vec::new();
        }

        builtin_symbol_suggestions(true)
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label == context_function_label)
            .collect()
    }

    pub fn from_workflow(workflow: &Workflow) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(workflow);
        let mut semantic_index = Self {
            providers: HashMap::new(),
            provider_locations: Vec::new(),
            schemas: HashMap::new(),
            schema_names: Vec::new(),
            schema_locations: Vec::new(),
            input_fields: BTreeMap::new(),
            input_field_metadata: BTreeMap::new(),
            secrets_fields: BTreeMap::new(),
            secrets_field_metadata: BTreeMap::new(),
            agents: HashMap::new(),
            agent_for_loop_iterators: HashMap::new(),
            agent_for_loop_iterator_types: HashMap::new(),
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
            semantic_index.insert_declaration(declaration);
        }

        semantic_index.schema_names.sort();
        semantic_index.schema_names.dedup();

        semantic_index.agent_names.sort();
        semantic_index.agent_names.dedup();

        semantic_index
    }

    fn insert_declaration(&mut self, declaration: &Declaration) {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                self.insert_provider(provider_declaration);
            }
            Declaration::Schema(schema_declaration) => {
                self.insert_schema_declaration(schema_declaration);
            }
            Declaration::Input(input_declaration) => {
                self.insert_input_declaration(input_declaration);
            }
            Declaration::Secrets(secrets_declaration) => {
                self.insert_secrets_declaration(secrets_declaration);
            }
            Declaration::Agent(agent_declaration) => {
                self.insert_agent_declaration(agent_declaration);
            }
            Declaration::Output(output_declaration) => {
                self.has_output_declaration = true;
                self.output_locations.push(output_declaration.span);
            }
        }
    }

    fn insert_schema_declaration(&mut self, schema_declaration: &superwire_core::dsl::SchemaDeclaration) {
        let schema_fields = schema_declaration
            .fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect::<BTreeMap<_, _>>();

        let schema_field_metadata = typed_fields_to_metadata_map(&schema_declaration.fields);

        self.schemas.insert(
            schema_declaration.name.clone(),
            SchemaSummary {
                fields: schema_fields,
                field_metadata: schema_field_metadata,
            },
        );

        self.schema_names.push(schema_declaration.name.clone());
        self.schema_locations.push(NamedSpan {
            name: schema_declaration.name.clone(),
            span: schema_declaration.span,
        });
        self.typed_declaration_locations.push(schema_declaration.span);
    }

    fn insert_input_declaration(&mut self, input_declaration: &superwire_core::dsl::InputDeclaration) {
        self.has_input_declaration = true;

        if self.input_fields.is_empty() {
            self.input_fields = typed_fields_to_map(&input_declaration.fields);
            self.input_field_metadata = typed_fields_to_metadata_map(&input_declaration.fields);
        }

        self.typed_declaration_locations.push(input_declaration.span);
    }

    fn insert_secrets_declaration(&mut self, secrets_declaration: &superwire_core::dsl::SecretsDeclaration) {
        self.has_secrets_declaration = true;

        if self.secrets_fields.is_empty() {
            self.secrets_fields = typed_fields_to_map(&secrets_declaration.fields);
            self.secrets_field_metadata = typed_fields_to_metadata_map(&secrets_declaration.fields);
        }

        self.typed_declaration_locations.push(secrets_declaration.span);
    }

    fn insert_agent_declaration(&mut self, agent_declaration: &superwire_core::dsl::AgentDeclaration) {
        let output_type = agent_declaration.properties.iter().find_map(|agent_property| match agent_property {
            AgentProperty::Output {
                output_type_expression,
                description: _,
            } => Some(output_type_expression.clone()),
            AgentProperty::Model(_)
            | AgentProperty::Prompt(_)
            | AgentProperty::Context(_)
            | AgentProperty::Inference(_)
            | AgentProperty::Tools(_) => None,
        });

        self.agents.insert(
            agent_declaration.name.clone(),
            AgentSummary {
                output_type: if agent_declaration.for_loop.is_some() {
                    output_type.map(|agent_output_type| TypeExpression::Array {
                        item_type: Box::new(agent_output_type),
                        fixed_length: None,
                    })
                } else {
                    output_type
                },
            },
        );

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            self.agent_for_loop_iterators
                .insert(agent_declaration.name.clone(), agent_for_loop.iterator_name.clone());

            if let Some(iterator_type) = self.iterable_item_type(&agent_for_loop.iterable) {
                self.agent_for_loop_iterator_types
                    .insert(agent_declaration.name.clone(), iterator_type);
            }
        }

        self.agent_names.push(agent_declaration.name.clone());
        self.agent_locations.push(NamedSpan {
            name: agent_declaration.name.clone(),
            span: agent_declaration.span,
        });
    }

    pub fn from_text_fallback(source_text: &str) -> Self {
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
                        field_metadata: field_metadata_from_type_map(schema_fields),
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
            input_field_metadata: field_metadata_from_type_map(tooling_snapshot.input_fields()),
            secrets_fields: tooling_snapshot.secrets_fields().clone(),
            secrets_field_metadata: field_metadata_from_type_map(tooling_snapshot.secrets_fields()),
            agents,
            agent_for_loop_iterators: HashMap::new(),
            agent_for_loop_iterator_types: HashMap::new(),
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

    pub fn model_call_suggestions(&self, model_call_context: &ModelCallCompletionContext) -> Vec<CompletionSuggestion> {
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

    pub fn provider_driver_value_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let provider_name = self.provider_name_at_position(position)?;
        let _ = provider_name;

        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, property_value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;

        if property_name_identifier != "driver" {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(property_value_prefix);
        let mut completion_suggestions = ProviderDriver::all()
            .into_iter()
            .map(superwire_core::ProviderDriver::as_str)
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

    pub fn provider_models_value_suggestions(&self, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, property_value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;

        if property_name_identifier != "models" {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(property_value_prefix);

        if value_completion_context.inside_string_literal {
            return Some(Vec::new());
        }

        let mut completion_suggestions = [
            CompletionSuggestion {
                label: "[]".to_string(),
                kind: CompletionKind::Value,
                detail: "Model list".to_string(),
                documentation: "Array of supported model identifiers.".to_string(),
                insert_text: "[]".to_string(),
            },
            CompletionSuggestion {
                label: "[\"\"]".to_string(),
                kind: CompletionKind::Value,
                detail: "Model list template".to_string(),
                documentation: "Array literal with one model placeholder string.".to_string(),
                insert_text: "[\"$1\"]".to_string(),
            },
        ]
        .into_iter()
        .filter(|completion_suggestion| completion_suggestion.label.starts_with(&value_completion_context.value_prefix))
        .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        Some(completion_suggestions)
    }

    pub fn provider_property_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
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

    pub fn is_type_position(&self, position: Position, line_prefix: &str) -> bool {
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

    pub fn type_suggestions(&self, line_prefix: &str, current_schema_name: Option<&str>) -> Vec<CompletionSuggestion> {
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
                    kind: CompletionKind::Type,
                    detail: "Named schema reference".to_string(),
                    documentation: "Reference a named schema type.".to_string(),
                    insert_text: format!("schema.{schema_name}"),
                }),
        );

        completion_suggestions
    }

    fn structural_type_suggestions(&self, type_prefix: &str) -> Vec<CompletionSuggestion> {
        let structural_type_suggestions = [
            CompletionSuggestion {
                label: "[string]".to_string(),
                kind: CompletionKind::Type,
                detail: "Array type".to_string(),
                documentation: "Array type expression.".to_string(),
                insert_text: "[string]".to_string(),
            },
            CompletionSuggestion {
                label: "{}".to_string(),
                kind: CompletionKind::Type,
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

    pub fn root_declaration_suggestions(&self, line_prefix: &str) -> Vec<CompletionSuggestion> {
        let declaration_prefix = trailing_identifier(line_prefix).unwrap_or_default();

        builtin_symbol_suggestions(false)
            .into_iter()
            .filter(|completion_suggestion| matches!(completion_suggestion.kind, CompletionKind::Keyword))
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(declaration_prefix))
            .filter(|completion_suggestion| self.should_suggest_root_declaration_label(&completion_suggestion.label))
            .collect()
    }

    pub fn is_output_position(&self, position: Position) -> bool {
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

    pub fn default_suggestions(&self, include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
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

    pub fn is_root_declaration_position(&self, position: Position) -> bool {
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

    pub fn provider_name_at_position(&self, position: Position) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| source_span_contains_position(provider_location.span, position))
            .map(|provider_location| provider_location.name.as_str())
    }

    pub fn schema_name_at_position(&self, position: Position) -> Option<&str> {
        self.schema_locations
            .iter()
            .find(|schema_location| source_span_contains_position(schema_location.span, position))
            .map(|schema_location| schema_location.name.as_str())
    }

    pub fn agent_name_at_position(&self, position: Position) -> Option<&str> {
        self.agent_locations
            .iter()
            .find(|agent_location| source_span_contains_position(agent_location.span, position))
            .map(|agent_location| agent_location.name.as_str())
    }

    pub(in crate::document) fn for_loop_iterator_name_at_position(&self, position: Position) -> Option<&str> {
        let agent_name = self.agent_name_at_position(position)?;

        self.agent_for_loop_iterators.get(agent_name).map(String::as_str)
    }

    pub fn for_loop_iterator_type_at_position(&self, position: Position) -> Option<&TypeExpression> {
        let agent_name = self.agent_name_at_position(position)?;

        self.agent_for_loop_iterator_types.get(agent_name)
    }

    fn iterable_item_type(&self, iterable_expression: &Expression) -> Option<TypeExpression> {
        let iterable_type = self.expression_type(iterable_expression)?;

        match iterable_type {
            TypeExpression::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            TypeExpression::Tuple(tuple_member_types) => Some(TypeExpression::Union(tuple_member_types)),
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::Object(_)
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Union(_) => None,
        }
    }

    fn expression_type(&self, expression: &Expression) -> Option<TypeExpression> {
        match expression {
            Expression::StringLiteral(_) | Expression::StringTemplate(_) => Some(TypeExpression::String),
            Expression::NumberLiteral(number_literal) => {
                if number_literal.contains('.') {
                    return Some(TypeExpression::Float);
                }

                Some(TypeExpression::Number)
            }
            Expression::BooleanLiteral(_) => Some(TypeExpression::Boolean),
            Expression::NullLiteral => Some(TypeExpression::Null),
            Expression::Reference(reference) => self.reference_expression_type(reference),
            Expression::FunctionCall(_) => None,
            Expression::ArrayLiteral(array_items) => {
                let mut array_item_types = array_items
                    .iter()
                    .filter_map(|array_item| self.expression_type(array_item))
                    .collect::<Vec<_>>();

                if array_item_types.is_empty() {
                    return None;
                }

                if array_item_types.len() == 1 {
                    return Some(TypeExpression::Array {
                        item_type: Box::new(array_item_types.remove(0)),
                        fixed_length: None,
                    });
                }

                Some(TypeExpression::Array {
                    item_type: Box::new(TypeExpression::Union(array_item_types)),
                    fixed_length: None,
                })
            }
            Expression::ObjectLiteral(object_fields) => {
                let typed_fields = object_fields
                    .iter()
                    .filter_map(|object_field| {
                        let field_type = self.expression_type(&object_field.value)?;

                        Some(TypedField {
                            name: object_field.name.clone(),
                            field_type,
                            description: None,
                            span: SourceSpan {
                                start: SourcePosition { line: 1, column: 1 },
                                end: SourcePosition { line: 1, column: 1 },
                            },
                        })
                    })
                    .collect::<Vec<_>>();

                Some(TypeExpression::Object(typed_fields))
            }
        }
    }

    fn reference_expression_type(&self, reference: &superwire_core::dsl::Reference) -> Option<TypeExpression> {
        let reference_keyword = reference.root_keyword()?;
        let reference_accesses = reference
            .accesses
            .iter()
            .map(|reference_access| reference_access.field.clone())
            .collect::<Vec<_>>();

        match reference_keyword {
            ReferenceKeyword::Input => self.resolve_singleton_reference_type(&self.input_fields, &reference_accesses),
            ReferenceKeyword::Secrets => self.resolve_singleton_reference_type(&self.secrets_fields, &reference_accesses),
            ReferenceKeyword::Agent => {
                let agent_name = reference_accesses.first()?;
                let agent_output_type = self.agents.get(agent_name)?.output_type.clone()?;

                if reference_accesses.len() == 1 {
                    return Some(agent_output_type);
                }

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_access_path_types(vec![agent_output_type], &reference_accesses[1..]);

                candidate_types.first().cloned()
            }
            ReferenceKeyword::Tool => None,
        }
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

fn typed_fields_to_metadata_map(typed_fields: &[TypedField]) -> BTreeMap<String, FieldMetadata> {
    typed_fields
        .iter()
        .map(|typed_field| {
            (
                typed_field.name.clone(),
                FieldMetadata {
                    field_type: typed_field.field_type.clone(),
                    description: typed_field.description.clone(),
                },
            )
        })
        .collect()
}

fn field_metadata_from_type_map(type_map: &BTreeMap<String, TypeExpression>) -> BTreeMap<String, FieldMetadata> {
    type_map
        .iter()
        .map(|(field_name, field_type)| {
            (
                field_name.clone(),
                FieldMetadata {
                    field_type: field_type.clone(),
                    description: None,
                },
            )
        })
        .collect()
}
