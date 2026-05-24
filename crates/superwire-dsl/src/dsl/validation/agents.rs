use super::super::ast::{
    AgentContext, AgentContextPropertyName, AgentDeclaration, AgentProperty, Declaration, ModelUsage, ModelUsagePropertyName, ObjectField,
    ReferenceKeyword, SourceSpan, Workflow,
};
use super::issues::AgentDeclarationIssuesExt;
use super::{ValidationIssue, ValidationReport};
use std::collections::HashSet;
use superwire_semantic::{InferenceSetting, WorkflowSemanticIndex as ValidationIndex};

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
                    validate_model_usage(agent_declaration, model_usage, validation_index, validation_report);
                }
                AgentProperty::Context(agent_context) => {
                    validate_compact_agent_context(agent_declaration, agent_context, validation_index, validation_report);
                }
                AgentProperty::InvalidModel(_) => {
                    has_model_property = true;
                    validation_report
                        .push_issue_with_span(agent_declaration.invalid_model_expression_issue(), Some(agent_declaration.span));
                }
                AgentProperty::Dynamic(_)
                | AgentProperty::Instruction(_)
                | AgentProperty::Output { fields: _, span: _ }
                | AgentProperty::Uses(_)
                | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        if !has_model_property {
            validation_report.push_issue_with_span(agent_declaration.invalid_model_expression_issue(), Some(agent_declaration.span));
        }
    }
}

fn validate_compact_agent_context(
    agent_declaration: &AgentDeclaration,
    agent_context: &AgentContext,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let AgentContext::Compact(compact_agent_context) = agent_context else {
        return;
    };

    for property in compact_agent_context.unsupported_properties() {
        validation_report.push_issue_with_span(
            ValidationIssue::UnsupportedAgentContextProperty {
                agent_name: agent_declaration.name.clone(),
                property_name: property.name.clone(),
            },
            Some(property.span),
        );
    }

    let Some(model_property) = compact_agent_context.property(AgentContextPropertyName::Model) else {
        return;
    };

    let Some(model_name) = compact_agent_context.model_name() else {
        validation_report.push_issue_with_span(agent_declaration.invalid_model_expression_issue(), Some(model_property.span));

        return;
    };

    if !validation_index.has_model(model_name) {
        validation_report.push_issue_with_span(agent_declaration.unknown_model_profile_issue(model_name), Some(model_property.span));
    }
}

fn validate_model_usage(
    agent_declaration: &AgentDeclaration,
    model_usage: &ModelUsage,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let Some(model_name) = model_usage.model_name() else {
        validation_report.push_issue_with_span(agent_declaration.invalid_model_expression_issue(), Some(model_usage.span));

        return;
    };

    if !validation_index.has_model(model_name) {
        validation_report.push_issue_with_span(
            agent_declaration.unknown_model_profile_issue(model_name),
            Some(model_usage.reference.span),
        );
    }

    for property in &model_usage.properties {
        if ModelUsagePropertyName::from_identifier(property.name.as_str()) == Some(ModelUsagePropertyName::Inference) {
            continue;
        }

        validation_report.push_issue_with_span(
            agent_declaration.invalid_model_usage_property_issue(&property.name),
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
                agent_declaration.unknown_tool_reference_issue(&issue_key.1),
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
                    agent_declaration.unknown_prompt_reference_issue(&issue_key.1),
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
                    agent_declaration.unknown_resource_reference_issue(&issue_key.1),
                    Some(agent_declaration.span),
                );
            }
        }
    }
}
