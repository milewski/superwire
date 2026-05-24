use super::super::ast::{
    AgentContext, AgentProperty, Asset, Declaration, Expression, MatchBranch, ObjectField, SourceSpan, StringTemplatePart, TypeExpression,
    TypedField, Workflow,
};
use super::issues::{AgentDeclarationIssuesExt, AgentPropertyIssuesExt, ObjectFieldIssuesExt, TypedFieldIssuesExt, VariantCaseIssuesExt};
use super::{ValidationContext, ValidationReport};
use std::collections::HashSet;

pub(super) trait WorkflowDuplicateValidationExt {
    fn validate_duplicate_properties(&self, validation_report: &mut ValidationReport);
}

impl WorkflowDuplicateValidationExt for Workflow {
    #[allow(clippy::too_many_lines)]
    fn validate_duplicate_properties(&self, validation_report: &mut ValidationReport) {
        let mut seen_workflow_dynamic_field_names = HashSet::<String>::new();

        for declaration in self.declarations() {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                    report_duplicate_object_field_names(
                        provider_declaration.properties.as_slice(),
                        provider_context.clone(),
                        Some(provider_declaration.span),
                        validation_report,
                    );

                    for provider_property in &provider_declaration.properties {
                        provider_property.value.report_duplicate_object_fields(
                            provider_context.clone(),
                            Some(provider_declaration.span),
                            validation_report,
                        );
                    }
                }
                Declaration::Model(model_declaration) => {
                    let model_context = ValidationContext::Model(model_declaration.name.clone());

                    report_duplicate_object_field_names(
                        model_declaration.properties.as_slice(),
                        model_context.clone(),
                        Some(model_declaration.span),
                        validation_report,
                    );

                    for model_property in &model_declaration.properties {
                        model_property.value.report_duplicate_object_fields(
                            model_context.clone(),
                            Some(model_declaration.span),
                            validation_report,
                        );
                    }
                }
                Declaration::McpServer(mcp_server_declaration) => {
                    report_duplicate_object_field_names(
                        mcp_server_declaration.properties.as_slice(),
                        ValidationContext::Provider(mcp_server_declaration.name.clone()),
                        Some(mcp_server_declaration.span),
                        validation_report,
                    );
                }
                Declaration::Schema(schema_declaration) => {
                    let schema_context = ValidationContext::Schema(schema_declaration.name.clone());

                    report_duplicate_typed_field_names(schema_declaration.fields.as_slice(), schema_context.clone(), validation_report);

                    for schema_field in &schema_declaration.fields {
                        schema_field
                            .field_type
                            .report_duplicate_fields(schema_context.clone(), validation_report);
                    }

                    if let Some(root_variant) = &schema_declaration.root_variant {
                        root_variant.report_duplicate_fields(schema_context, validation_report);
                    }
                }
                Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                        report_duplicate_typed_field_names(
                            tool_declaration.input_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );
                        report_duplicate_typed_field_names(
                            tool_declaration.binding_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );
                        report_duplicate_typed_field_names(
                            tool_declaration.output_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );

                        for input_field in &tool_declaration.input_fields {
                            input_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }

                        for binding_field in &tool_declaration.binding_fields {
                            binding_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }

                        for output_field in &tool_declaration.output_fields {
                            output_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                        report_duplicate_typed_field_names(
                            tool_declaration.input_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );
                        report_duplicate_typed_field_names(
                            tool_declaration.binding_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );
                        report_duplicate_typed_field_names(
                            tool_declaration.output_fields.as_slice(),
                            tool_context.clone(),
                            validation_report,
                        );

                        for input_field in &tool_declaration.input_fields {
                            input_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }

                        for binding_field in &tool_declaration.binding_fields {
                            binding_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }

                        for output_field in &tool_declaration.output_fields {
                            output_field
                                .field_type
                                .report_duplicate_fields(tool_context.clone(), validation_report);
                        }
                    }

                    for resource_import_declaration in &batch_import_declaration.resources {
                        let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                        report_duplicate_object_field_names(
                            resource_import_declaration.parameters.as_slice(),
                            resource_context.clone(),
                            Some(resource_import_declaration.span),
                            validation_report,
                        );

                        for parameter in &resource_import_declaration.parameters {
                            parameter.value.report_duplicate_object_fields(
                                resource_context.clone(),
                                Some(resource_import_declaration.span),
                                validation_report,
                            );
                        }
                    }

                    for prompt_import_declaration in &batch_import_declaration.prompts {
                        let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                        report_duplicate_object_field_names(
                            prompt_import_declaration.parameters.as_slice(),
                            prompt_context.clone(),
                            Some(prompt_import_declaration.span),
                            validation_report,
                        );

                        for parameter in &prompt_import_declaration.parameters {
                            parameter.value.report_duplicate_object_fields(
                                prompt_context.clone(),
                                Some(prompt_import_declaration.span),
                                validation_report,
                            );
                        }
                    }
                }
                Declaration::McpResource(resource_import_declaration) => {
                    let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                    report_duplicate_object_field_names(
                        resource_import_declaration.parameters.as_slice(),
                        resource_context.clone(),
                        Some(resource_import_declaration.span),
                        validation_report,
                    );

                    for parameter in &resource_import_declaration.parameters {
                        parameter.value.report_duplicate_object_fields(
                            resource_context.clone(),
                            Some(resource_import_declaration.span),
                            validation_report,
                        );
                    }
                }
                Declaration::McpPrompt(prompt_import_declaration) => {
                    let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                    report_duplicate_object_field_names(
                        prompt_import_declaration.parameters.as_slice(),
                        prompt_context.clone(),
                        Some(prompt_import_declaration.span),
                        validation_report,
                    );

                    for parameter in &prompt_import_declaration.parameters {
                        parameter.value.report_duplicate_object_fields(
                            prompt_context.clone(),
                            Some(prompt_import_declaration.span),
                            validation_report,
                        );
                    }
                }
                Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                    for resource_import_declaration in &resource_batch_import_declaration.resources {
                        let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                        report_duplicate_object_field_names(
                            resource_import_declaration.parameters.as_slice(),
                            resource_context.clone(),
                            Some(resource_import_declaration.span),
                            validation_report,
                        );

                        for parameter in &resource_import_declaration.parameters {
                            parameter.value.report_duplicate_object_fields(
                                resource_context.clone(),
                                Some(resource_import_declaration.span),
                                validation_report,
                            );
                        }
                    }
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                        let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                        report_duplicate_object_field_names(
                            prompt_import_declaration.parameters.as_slice(),
                            prompt_context.clone(),
                            Some(prompt_import_declaration.span),
                            validation_report,
                        );

                        for parameter in &prompt_import_declaration.parameters {
                            parameter.value.report_duplicate_object_fields(
                                prompt_context.clone(),
                                Some(prompt_import_declaration.span),
                                validation_report,
                            );
                        }
                    }
                }
                Declaration::Agent(agent_declaration) => {
                    let agent_context = ValidationContext::Agent(agent_declaration.name.clone());
                    let mut seen_agent_properties = HashSet::<String>::new();
                    let mut seen_agent_dynamic_field_names = HashSet::<String>::new();

                    for agent_property in &agent_declaration.properties {
                        if let Some(agent_property_name) = agent_property.name() {
                            if !agent_property.repeatable() && !seen_agent_properties.insert(agent_property_name.to_string()) {
                                let Some(validation_issue) = agent_property.duplicate_property_issue(agent_context.clone()) else {
                                    continue;
                                };

                                validation_report.push_issue_with_span(validation_issue, Some(agent_declaration.span));
                            }
                        }

                        match agent_property {
                            AgentProperty::Dynamic(dynamic_block) => {
                                report_duplicate_object_field_names(
                                    dynamic_block.fields.as_slice(),
                                    agent_context.clone(),
                                    Some(dynamic_block.span),
                                    validation_report,
                                );

                                for dynamic_field in &dynamic_block.fields {
                                    if !seen_agent_dynamic_field_names.insert(dynamic_field.name.clone()) {
                                        validation_report.push_issue_with_span(
                                            dynamic_field.duplicate_property_issue(agent_context.clone()),
                                            Some(dynamic_block.span),
                                        );
                                    }

                                    dynamic_field.value.report_duplicate_object_fields(
                                        agent_context.clone(),
                                        Some(dynamic_block.span),
                                        validation_report,
                                    );
                                }
                            }
                            AgentProperty::InvalidModel(expression)
                            | AgentProperty::Instruction(expression)
                            | AgentProperty::Uses(expression) => {
                                expression.report_duplicate_object_fields(
                                    agent_context.clone(),
                                    Some(agent_declaration.span),
                                    validation_report,
                                );
                            }
                            AgentProperty::Context(context_value) => {
                                if let AgentContext::Compact(compact_agent_context) = context_value {
                                    report_duplicate_object_field_names(
                                        compact_agent_context.properties.as_slice(),
                                        agent_context.clone(),
                                        Some(compact_agent_context.span),
                                        validation_report,
                                    );
                                }
                            }
                            AgentProperty::Model(model_usage) => {
                                report_duplicate_object_field_names(
                                    model_usage.properties.as_slice(),
                                    agent_context.clone(),
                                    Some(model_usage.span),
                                    validation_report,
                                );

                                for model_property in &model_usage.properties {
                                    model_property.value.report_duplicate_object_fields(
                                        agent_context.clone(),
                                        Some(model_usage.span),
                                        validation_report,
                                    );
                                }
                            }
                            AgentProperty::Unknown { name, span } => {
                                validation_report.push_issue_with_span(agent_declaration.unknown_property_issue(name), Some(*span));
                            }
                            AgentProperty::Output { fields, span: _ } => {
                                report_duplicate_typed_field_names(fields.as_slice(), agent_context.clone(), validation_report);

                                for output_field in fields {
                                    output_field
                                        .field_type
                                        .report_duplicate_fields(agent_context.clone(), validation_report);
                                }
                            }
                        }
                    }
                }
                Declaration::Input(input_declaration) => {
                    let input_context = ValidationContext::Input;

                    report_duplicate_typed_field_names(input_declaration.fields.as_slice(), input_context.clone(), validation_report);

                    for input_field in &input_declaration.fields {
                        input_field
                            .field_type
                            .report_duplicate_fields(input_context.clone(), validation_report);
                    }
                }
                Declaration::Secrets(secrets_declaration) => {
                    let secrets_context = ValidationContext::Secrets;

                    report_duplicate_typed_field_names(secrets_declaration.fields.as_slice(), secrets_context.clone(), validation_report);

                    for secrets_field in &secrets_declaration.fields {
                        secrets_field
                            .field_type
                            .report_duplicate_fields(secrets_context.clone(), validation_report);
                    }
                }
                Declaration::Output(output_declaration) => {
                    let output_context = ValidationContext::Output;

                    report_duplicate_object_field_names(
                        output_declaration.fields.as_slice(),
                        output_context.clone(),
                        Some(output_declaration.span),
                        validation_report,
                    );

                    for output_field in &output_declaration.fields {
                        output_field.value.report_duplicate_object_fields(
                            output_context.clone(),
                            Some(output_declaration.span),
                            validation_report,
                        );
                    }
                }
                Declaration::Dynamic(dynamic_block) => {
                    report_duplicate_object_field_names(
                        dynamic_block.fields.as_slice(),
                        ValidationContext::Dynamic,
                        Some(dynamic_block.span),
                        validation_report,
                    );

                    for dynamic_field in &dynamic_block.fields {
                        if !seen_workflow_dynamic_field_names.insert(dynamic_field.name.clone()) {
                            validation_report.push_issue_with_span(
                                dynamic_field.duplicate_property_issue(ValidationContext::Dynamic),
                                Some(dynamic_block.span),
                            );
                        }

                        dynamic_field.value.report_duplicate_object_fields(
                            ValidationContext::Dynamic,
                            Some(dynamic_block.span),
                            validation_report,
                        );
                    }
                }
            }
        }
    }
}

fn report_duplicate_object_field_names(
    object_fields: &[ObjectField],
    context: ValidationContext,
    duplicate_span: Option<SourceSpan>,
    validation_report: &mut ValidationReport,
) {
    let mut seen_field_names = HashSet::<String>::new();

    for object_field in object_fields {
        if seen_field_names.insert(object_field.name.clone()) {
            continue;
        }

        validation_report.push_issue_with_span(object_field.duplicate_property_issue(context.clone()), duplicate_span);
    }
}

fn report_duplicate_typed_field_names(typed_fields: &[TypedField], context: ValidationContext, validation_report: &mut ValidationReport) {
    let mut seen_field_names = HashSet::<String>::new();

    for typed_field in typed_fields {
        if seen_field_names.insert(typed_field.name.clone()) {
            continue;
        }

        validation_report.push_issue_with_span(typed_field.duplicate_property_issue(context.clone()), Some(typed_field.span));
    }
}

trait TypeExpressionDuplicateValidationExt {
    fn report_duplicate_fields(&self, context: ValidationContext, validation_report: &mut ValidationReport);
}

impl TypeExpressionDuplicateValidationExt for TypeExpression {
    fn report_duplicate_fields(&self, context: ValidationContext, validation_report: &mut ValidationReport) {
        match self {
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                item_type.report_duplicate_fields(context, validation_report);
            }
            Self::Tuple(tuple_items) | Self::Union(tuple_items) => {
                for tuple_item in tuple_items {
                    tuple_item.report_duplicate_fields(context.clone(), validation_report);
                }
            }
            Self::Object(typed_fields) => {
                report_duplicate_typed_field_names(typed_fields.as_slice(), context.clone(), validation_report);

                for typed_field in typed_fields {
                    typed_field.field_type.report_duplicate_fields(context.clone(), validation_report);
                }
            }
            Self::Variant { discriminator, cases } => {
                for variant_case in cases {
                    report_duplicate_typed_field_names(variant_case.fields.as_slice(), context.clone(), validation_report);

                    for typed_field in &variant_case.fields {
                        if typed_field.name == *discriminator {
                            validation_report.push_issue_with_span(
                                variant_case.duplicate_discriminator_field_issue(discriminator),
                                Some(typed_field.span),
                            );
                        }

                        typed_field.field_type.report_duplicate_fields(context.clone(), validation_report);
                    }
                }
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_) => {}
        }
    }
}

trait ExpressionDuplicateValidationExt {
    fn report_duplicate_object_fields(
        &self,
        context: ValidationContext,
        duplicate_span: Option<SourceSpan>,
        validation_report: &mut ValidationReport,
    );
}

impl ExpressionDuplicateValidationExt for Expression {
    fn report_duplicate_object_fields(
        &self,
        context: ValidationContext,
        duplicate_span: Option<SourceSpan>,
        validation_report: &mut ValidationReport,
    ) {
        match self {
            Self::FunctionCall(function_call) => {
                for call_argument in &function_call.arguments {
                    call_argument
                        .expression()
                        .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::Asset(asset) => {
                asset.report_duplicate_object_fields(context, duplicate_span, validation_report);
            }
            Self::ToolCall(tool_call) => {
                report_duplicate_object_field_names(
                    tool_call.input_fields.as_slice(),
                    context.clone(),
                    duplicate_span,
                    validation_report,
                );
                report_duplicate_object_field_names(
                    tool_call.binding_fields.as_slice(),
                    context.clone(),
                    duplicate_span,
                    validation_report,
                );

                for object_field in &tool_call.input_fields {
                    object_field
                        .value
                        .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }

                for object_field in &tool_call.binding_fields {
                    object_field
                        .value
                        .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::McpCall(mcp_call) => {
                report_duplicate_object_field_names(
                    mcp_call.parameter_fields.as_slice(),
                    context.clone(),
                    duplicate_span,
                    validation_report,
                );

                for object_field in &mcp_call.parameter_fields {
                    object_field
                        .value
                        .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback
                    .value
                    .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                null_fallback
                    .fallback
                    .report_duplicate_object_fields(context, duplicate_span, validation_report);
            }
            Self::Match(match_expression) => {
                match_expression
                    .value
                    .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        value.report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                    }
                }
            }
            Self::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    array_value.report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                report_duplicate_object_field_names(object_fields.as_slice(), context.clone(), duplicate_span, validation_report);

                for object_field in object_fields {
                    object_field
                        .value
                        .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part else {
                        continue;
                    };

                    interpolation_expression.report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
                }
            }
            Self::StringLiteral(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::VariantProjection(_)
            | Self::Reference(_) => {}
        }
    }
}

trait AssetDuplicateValidationExt {
    fn report_duplicate_object_fields(
        &self,
        context: ValidationContext,
        duplicate_span: Option<SourceSpan>,
        validation_report: &mut ValidationReport,
    );
}

impl AssetDuplicateValidationExt for Asset {
    fn report_duplicate_object_fields(
        &self,
        context: ValidationContext,
        duplicate_span: Option<SourceSpan>,
        validation_report: &mut ValidationReport,
    ) {
        report_duplicate_object_field_names(self.options.as_slice(), context.clone(), duplicate_span, validation_report);
        self.source
            .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);

        for option in &self.options {
            option
                .value
                .report_duplicate_object_fields(context.clone(), duplicate_span, validation_report);
        }
    }
}
