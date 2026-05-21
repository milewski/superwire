use super::super::ast::{
    AgentProperty, Declaration, ModelUsage, ModelUsagePropertyName, ObjectField, ReferenceKeyword, SourceSpan, Workflow,
};
use super::report::{ValidationContext, ValidationIssue, ValidationReport};
use crate::semantic::{InferenceSetting, WorkflowSemanticIndex as ValidationIndex};
use std::collections::HashSet;

pub(super) fn validate_agent_inference_settings(workflow: &Workflow, validation_report: &mut ValidationReport) {
    let mut invalid_inference_setting_values = HashSet::<(String, InferenceSetting)>::new();

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Model(model_declaration) => {
                if let Some(inference_fields) = model_declaration.inference_fields() {
                    validate_inference_fields(
                        &model_declaration.name,
                        inference_fields,
                        Some(model_declaration.span),
                        &mut invalid_inference_setting_values,
                        validation_report,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                for agent_property in &agent_declaration.properties {
                    match agent_property {
                        AgentProperty::Model(model_usage) => {
                            if let Some(inference_fields) = model_usage.inference_fields() {
                                validate_inference_fields(
                                    &agent_declaration.name,
                                    inference_fields,
                                    Some(model_usage.span),
                                    &mut invalid_inference_setting_values,
                                    validation_report,
                                );
                            }
                        }
                        AgentProperty::Dynamic(_)
                        | AgentProperty::InvalidModel(_)
                        | AgentProperty::Instruction(_)
                        | AgentProperty::Output { fields: _, span: _ }
                        | AgentProperty::Context(_)
                        | AgentProperty::Uses(_)
                        | AgentProperty::Unknown { name: _, span: _ } => {}
                    }
                }
            }
            Declaration::Provider(_)
            | Declaration::McpServer(_)
            | Declaration::Secrets(_)
            | Declaration::Input(_)
            | Declaration::Schema(_)
            | Declaration::Tool(_)
            | Declaration::McpBatch(_)
            | Declaration::McpToolBatch(_)
            | Declaration::McpResourceBatch(_)
            | Declaration::McpPromptBatch(_)
            | Declaration::McpResource(_)
            | Declaration::McpPrompt(_)
            | Declaration::Dynamic(_)
            | Declaration::Output(_) => {}
        }
    }
}

fn validate_inference_fields(
    owner_name: &str,
    inference_fields: &[ObjectField],
    span: Option<SourceSpan>,
    invalid_inference_setting_values: &mut HashSet<(String, InferenceSetting)>,
    validation_report: &mut ValidationReport,
) {
    for inference_field in inference_fields {
        let Some(inference_setting) = InferenceSetting::from_identifier(inference_field.name.as_str()) else {
            continue;
        };

        if inference_setting.accepts_expression(&inference_field.value) {
            continue;
        }

        let issue_key = (owner_name.to_string(), inference_setting);

        if !invalid_inference_setting_values.insert(issue_key.clone()) {
            continue;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name: issue_key.0,
                inference_setting: issue_key.1,
            },
            span,
        );
    }
}

pub(super) fn validate_agent_model_bindings(
    workflow: &Workflow,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let mut has_model_property = false;

        for agent_property in &agent_declaration.properties {
            match agent_property {
                AgentProperty::Model(model_usage) => {
                    has_model_property = true;
                    validate_model_usage(&agent_declaration.name, model_usage, validation_index, validation_report);
                }
                AgentProperty::InvalidModel(_) => {
                    has_model_property = true;
                    validation_report.push_issue_with_span(
                        ValidationIssue::InvalidModelExpression {
                            agent_name: agent_declaration.name.clone(),
                        },
                        Some(agent_declaration.span),
                    );
                }
                AgentProperty::Dynamic(_)
                | AgentProperty::Instruction(_)
                | AgentProperty::Output { fields: _, span: _ }
                | AgentProperty::Context(_)
                | AgentProperty::Uses(_)
                | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        if !has_model_property {
            validation_report.push_issue_with_span(
                ValidationIssue::InvalidModelExpression {
                    agent_name: agent_declaration.name.clone(),
                },
                Some(agent_declaration.span),
            );
        }
    }
}

fn validate_model_usage(
    agent_name: &str,
    model_usage: &ModelUsage,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let Some(model_name) = model_usage.model_name() else {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            Some(model_usage.span),
        );

        return;
    };

    if !validation_index.has_model(model_name) {
        validation_report.push_issue_with_span(
            ValidationIssue::UnknownModelProfile {
                agent_name: agent_name.to_owned(),
                model_name: model_name.to_owned(),
            },
            Some(model_usage.reference.span),
        );
    }

    for property in &model_usage.properties {
        if ModelUsagePropertyName::from_identifier(property.name.as_str()) == Some(ModelUsagePropertyName::Inference) {
            continue;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelUsageProperty {
                agent_name: agent_name.to_owned(),
                property_name: property.name.clone(),
            },
            Some(property.span),
        );
    }
}

pub(super) fn validate_agent_tool_references(
    workflow: &Workflow,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let mut reported_unknown_tools = HashSet::<(String, String)>::new();
    let mut reported_unknown_prompts = HashSet::<(String, String)>::new();
    let mut reported_unknown_resources = HashSet::<(String, String)>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let Some(uses_expression) = agent_declaration.expression_property(crate::dsl::AgentExpressionPropertyName::Uses) else {
            continue;
        };

        for tool_reference in uses_expression.tool_references() {
            let Some(tool_name) = tool_reference.tool_name() else {
                continue;
            };

            if validation_index.has_tool(tool_name) {
                continue;
            }

            let issue_key = (agent_declaration.name.clone(), tool_name.to_string());

            if !reported_unknown_tools.insert(issue_key.clone()) {
                continue;
            }

            validation_report.push_issue_with_span(
                ValidationIssue::UnknownToolReference {
                    agent_name: issue_key.0,
                    tool_name: issue_key.1,
                },
                Some(agent_declaration.span),
            );
        }

        for prompt_name in uses_expression.referenced_names_for_keyword(ReferenceKeyword::Prompt) {
            if validation_index.has_prompt(&prompt_name) {
                continue;
            }

            let issue_key = (agent_declaration.name.clone(), prompt_name.clone());

            if reported_unknown_prompts.insert(issue_key.clone()) {
                validation_report.push_issue_with_span(
                    ValidationIssue::UnknownPromptReference {
                        prompt_name: issue_key.1,
                        context: ValidationContext::Agent(issue_key.0),
                    },
                    Some(agent_declaration.span),
                );
            }
        }

        for resource_name in uses_expression.referenced_names_for_keyword(ReferenceKeyword::Resource) {
            if validation_index.has_resource(&resource_name) {
                continue;
            }

            let issue_key = (agent_declaration.name.clone(), resource_name.clone());

            if reported_unknown_resources.insert(issue_key.clone()) {
                validation_report.push_issue_with_span(
                    ValidationIssue::UnknownResourceReference {
                        resource_name: issue_key.1,
                        context: ValidationContext::Agent(issue_key.0),
                    },
                    Some(agent_declaration.span),
                );
            }
        }
    }
}
