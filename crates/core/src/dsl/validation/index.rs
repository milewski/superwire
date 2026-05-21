use super::super::ast::{Declaration, ObjectField, SourceSpan, TypeExpression, TypedField, Workflow};
use super::report::{SingletonDeclarationKind, ValidationIssue, ValidationReport};
use crate::semantic::support::types::{workflow_type_from_dsl, WorkflowType};
use crate::semantic::ProviderDriver;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub(super) struct ValidationIndex {
    pub(super) provider_names: HashSet<String>,
    pub(super) model_names: HashSet<String>,
    pub(super) agent_names: HashSet<String>,
    pub(super) tool_names: HashSet<String>,
    pub(super) resource_names: HashSet<String>,
    pub(super) prompt_names: HashSet<String>,
    pub(super) schema_names: HashSet<String>,
    schema_field_types: HashMap<String, HashMap<String, TypeExpression>>,
    schema_types: HashMap<String, TypeExpression>,
    pub(super) input_field_types: Option<HashMap<String, TypeExpression>>,
    pub(super) secrets_field_types: Option<HashMap<String, TypeExpression>>,
    pub(super) agent_output_types: HashMap<String, Option<TypeExpression>>,
    pub(super) tool_input_types: HashMap<String, WorkflowType>,
    pub(super) tool_binding_types: HashMap<String, WorkflowType>,
    pub(super) tool_fixed_binding_names: HashMap<String, HashSet<String>>,
    pub(super) tool_fixed_binding_fields: HashMap<String, Vec<ObjectField>>,
    pub(super) tool_output_types: HashMap<String, WorkflowType>,
}

impl ValidationIndex {
    pub(super) fn schema_type_expression(&self, schema_name: &str, span: SourceSpan) -> Option<TypeExpression> {
        if let Some(schema_type) = self.schema_types.get(schema_name) {
            return Some(schema_type.clone());
        }

        let schema_field_types = self.schema_field_types.get(schema_name)?;
        let typed_fields = schema_field_types
            .iter()
            .map(|(field_name, field_type)| TypedField {
                name: field_name.clone(),
                field_type: field_type.clone(),
                description: None,
                span,
            })
            .collect::<Vec<_>>();

        Some(TypeExpression::Object(typed_fields))
    }

    pub(super) fn named_schema_types(&self, span: SourceSpan) -> HashMap<String, TypeExpression> {
        self.schema_names
            .iter()
            .filter_map(|schema_name| {
                self.schema_type_expression(schema_name, span)
                    .map(|schema_type| (schema_name.clone(), schema_type))
            })
            .collect()
    }
}

impl ValidationIndex {
    #[allow(clippy::too_many_lines)]
    pub(super) fn build(workflow: &Workflow, validation_report: &mut ValidationReport) -> Self {
        let mut validation_index = Self::default();

        let mut has_input_declaration = false;
        let mut has_secrets_declaration = false;
        let mut has_output_declaration = false;

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    let provider_name = provider_declaration.name.clone();

                    if !validation_index.register_provider_name(provider_declaration, validation_report) {
                        continue;
                    }

                    let provider_driver = ProviderDriver::parse(&provider_declaration.driver_name);

                    if provider_driver.is_none() {
                        validation_report.push_issue_with_span(
                            ValidationIssue::UnknownProviderDriver {
                                provider_name: provider_name.clone(),
                                driver_name: provider_declaration.driver_name.clone(),
                            },
                            Some(provider_declaration.span),
                        );
                    }
                }
                Declaration::Model(model_declaration) => {
                    let model_name = model_declaration.name.clone();

                    if !validation_index.register_model_name(model_declaration, validation_report) {
                        continue;
                    }

                    if !validation_index.provider_names.contains(&model_declaration.provider_name) {
                        validation_report.push_issue_with_span(
                            ValidationIssue::UnknownProviderInModelDeclaration {
                                model_name: model_name.clone(),
                                provider_name: model_declaration.provider_name.clone(),
                            },
                            Some(model_declaration.span),
                        );
                    }

                    if model_declaration.id_expression().is_none() {
                        validation_report.push_issue_with_span(
                            ValidationIssue::MissingModelId {
                                model_name: model_name.clone(),
                            },
                            Some(model_declaration.span),
                        );
                    }
                }
                Declaration::McpServer(_) => {}
                Declaration::Schema(schema_declaration) => {
                    if !validation_index.register_schema_name(schema_declaration, validation_report) {
                        continue;
                    }

                    let schema_field_types = Self::collect_field_types(schema_declaration.fields.as_slice());
                    validation_index
                        .schema_field_types
                        .insert(schema_declaration.name.clone(), schema_field_types);
                    validation_index
                        .schema_types
                        .insert(schema_declaration.name.clone(), schema_declaration.type_expression());
                }
                Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let named_schema_types = validation_index.named_schema_types(tool_declaration.span);

                        if let Ok(tool_input_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_input_types
                                .insert(tool_declaration.name.clone(), tool_input_type);
                        }

                        if let Ok(tool_binding_type) = workflow_type_from_dsl(
                            &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                            &named_schema_types,
                        ) {
                            validation_index
                                .tool_binding_types
                                .insert(tool_declaration.name.clone(), tool_binding_type);
                        }

                        let fixed_binding_names = tool_declaration
                            .fixed_binding_fields
                            .iter()
                            .map(|fixed_binding| fixed_binding.name.clone())
                            .collect::<HashSet<_>>();

                        if !fixed_binding_names.is_empty() {
                            validation_index
                                .tool_fixed_binding_names
                                .insert(tool_declaration.name.clone(), fixed_binding_names);
                        }

                        if !tool_declaration.fixed_binding_fields.is_empty() {
                            validation_index
                                .tool_fixed_binding_fields
                                .insert(tool_declaration.name.clone(), tool_declaration.fixed_binding_fields.clone());
                        }

                        if tool_declaration.has_untyped_mcp_output() {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
                        } else if let Ok(tool_output_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), tool_output_type);
                        }

                        validation_index.register_tool_name(tool_declaration, validation_report);
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let named_schema_types = validation_index.named_schema_types(tool_declaration.span);

                        if let Ok(tool_input_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_input_types
                                .insert(tool_declaration.name.clone(), tool_input_type);
                        }

                        if let Ok(tool_binding_type) = workflow_type_from_dsl(
                            &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                            &named_schema_types,
                        ) {
                            validation_index
                                .tool_binding_types
                                .insert(tool_declaration.name.clone(), tool_binding_type);
                        }

                        let fixed_binding_names = tool_declaration
                            .fixed_binding_fields
                            .iter()
                            .map(|fixed_binding| fixed_binding.name.clone())
                            .collect::<HashSet<_>>();

                        if !fixed_binding_names.is_empty() {
                            validation_index
                                .tool_fixed_binding_names
                                .insert(tool_declaration.name.clone(), fixed_binding_names);
                        }

                        if !tool_declaration.fixed_binding_fields.is_empty() {
                            validation_index
                                .tool_fixed_binding_fields
                                .insert(tool_declaration.name.clone(), tool_declaration.fixed_binding_fields.clone());
                        }

                        if tool_declaration.has_untyped_mcp_output() {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
                        } else if let Ok(tool_output_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), tool_output_type);
                        }

                        validation_index.register_tool_name(tool_declaration, validation_report);
                    }

                    for resource_import_declaration in &batch_import_declaration.resources {
                        validation_index.register_resource_name(resource_import_declaration, validation_report);
                    }

                    for prompt_import_declaration in &batch_import_declaration.prompts {
                        validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                    }
                }
                Declaration::McpResource(resource_import_declaration) => {
                    validation_index.register_resource_name(resource_import_declaration, validation_report);
                }
                Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                    for resource_import_declaration in &resource_batch_import_declaration.resources {
                        validation_index.register_resource_name(resource_import_declaration, validation_report);
                    }
                }
                Declaration::McpPrompt(prompt_import_declaration) => {
                    validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                        validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                    }
                }
                Declaration::Dynamic(_) => {}
                Declaration::Agent(agent_declaration) => {
                    if !validation_index.register_agent_name(agent_declaration, validation_report) {
                        continue;
                    }

                    let agent_output_type = agent_declaration.declared_final_output_type_expression();
                    validation_index
                        .agent_output_types
                        .insert(agent_declaration.name.clone(), agent_output_type);
                }
                Declaration::Input(input_declaration) => {
                    if has_input_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Input,
                            },
                            Some(input_declaration.span),
                        );
                    }

                    has_input_declaration = true;

                    if validation_index.input_field_types.is_none() {
                        validation_index.input_field_types = Some(Self::collect_field_types(input_declaration.fields.as_slice()));
                    }
                }
                Declaration::Secrets(secrets_declaration) => {
                    if has_secrets_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Secrets,
                            },
                            Some(secrets_declaration.span),
                        );
                    }

                    has_secrets_declaration = true;

                    if validation_index.secrets_field_types.is_none() {
                        validation_index.secrets_field_types = Some(Self::collect_field_types(secrets_declaration.fields.as_slice()));
                    }
                }
                Declaration::Output(output_declaration) => {
                    if has_output_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Output,
                            },
                            Some(output_declaration.span),
                        );
                    }

                    has_output_declaration = true;
                }
            }
        }

        validation_index
    }

    fn collect_field_types(typed_fields: &[TypedField]) -> HashMap<String, TypeExpression> {
        typed_fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect()
    }
}
