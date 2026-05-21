use super::super::ast::{AgentProperty, Declaration, Reference, ReferenceKeyword, SourceSpan, TypeExpression, Workflow};
use super::report::{ValidationContext, ValidationIssue, ValidationReport};
use crate::semantic::WorkflowSemanticIndex as ValidationIndex;
use std::collections::HashSet;

pub(super) fn validate_schema_references(
    workflow: &Workflow,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let mut validation_state = SchemaValidationState::new(validation_index, validation_report);

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Input(input_declaration) => {
                for typed_field in &input_declaration.fields {
                    typed_field
                        .field_type
                        .validate_for_schemas(ValidationContext::Input, Some(typed_field.span), &mut validation_state);
                }
            }
            Declaration::Secrets(secrets_declaration) => {
                for typed_field in &secrets_declaration.fields {
                    typed_field
                        .field_type
                        .validate_for_schemas(ValidationContext::Secrets, Some(typed_field.span), &mut validation_state);
                }
            }
            Declaration::Schema(schema_declaration) => {
                let schema_context = ValidationContext::Schema(schema_declaration.name.clone());

                for typed_field in &schema_declaration.fields {
                    typed_field
                        .field_type
                        .validate_for_schemas(schema_context.clone(), Some(typed_field.span), &mut validation_state);
                }

                if let Some(root_variant) = &schema_declaration.root_variant {
                    root_variant.validate_for_schemas(schema_context, Some(schema_declaration.span), &mut validation_state);
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                for agent_property in &agent_declaration.properties {
                    let AgentProperty::Output { fields, span: _ } = agent_property else {
                        continue;
                    };

                    for output_field in fields {
                        output_field
                            .field_type
                            .validate_for_schemas(agent_context.clone(), Some(output_field.span), &mut validation_state);
                    }
                }
            }
            Declaration::Tool(_) | Declaration::McpBatch(_) | Declaration::McpToolBatch(_) => {
                for tool_declaration in declaration.tool_declarations() {
                    let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                    for input_field in &tool_declaration.input_fields {
                        input_field
                            .field_type
                            .validate_for_schemas(tool_context.clone(), Some(input_field.span), &mut validation_state);
                    }

                    for binding_field in &tool_declaration.binding_fields {
                        binding_field.field_type.validate_for_schemas(
                            tool_context.clone(),
                            Some(binding_field.span),
                            &mut validation_state,
                        );
                    }

                    for output_field in &tool_declaration.output_fields {
                        output_field
                            .field_type
                            .validate_for_schemas(tool_context.clone(), Some(output_field.span), &mut validation_state);
                    }
                }
            }
            Declaration::Provider(_)
            | Declaration::Model(_)
            | Declaration::McpServer(_)
            | Declaration::McpResource(_)
            | Declaration::McpPrompt(_)
            | Declaration::McpResourceBatch(_)
            | Declaration::McpPromptBatch(_)
            | Declaration::Dynamic(_)
            | Declaration::Output(_) => {}
        }
    }
}

struct SchemaValidationState<'validation> {
    validation_index: &'validation ValidationIndex,
    validation_report: &'validation mut ValidationReport,
    unknown_schema_references: HashSet<(ValidationContext, String)>,
    invalid_type_expression_references: HashSet<(ValidationContext, String)>,
}

impl<'validation> SchemaValidationState<'validation> {
    fn new(validation_index: &'validation ValidationIndex, validation_report: &'validation mut ValidationReport) -> Self {
        Self {
            validation_index,
            validation_report,
            unknown_schema_references: HashSet::new(),
            invalid_type_expression_references: HashSet::new(),
        }
    }

    fn push_unknown_schema_reference(&mut self, referenced_schema: String, context: ValidationContext, span: Option<SourceSpan>) {
        let issue_key = (context.clone(), referenced_schema.clone());

        if self.unknown_schema_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::UnknownSchemaReference {
                    referenced_schema,
                    context,
                },
                span,
            );
        }
    }
}

impl TypeExpression {
    fn validate_for_schemas(&self, context: ValidationContext, span: Option<SourceSpan>, validation_state: &mut SchemaValidationState) {
        match self {
            Self::SchemaReference(referenced_schema_name) => {
                if validation_state.validation_index.has_schema(referenced_schema_name) {
                    return;
                }

                validation_state.push_unknown_schema_reference(referenced_schema_name.clone(), context, span);
            }
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                item_type.validate_for_schemas(context, span, validation_state);
            }
            Self::Tuple(type_expressions) | Self::Union(type_expressions) => {
                for nested_type_expression in type_expressions {
                    nested_type_expression.validate_for_schemas(context.clone(), span, validation_state);
                }
            }
            Self::Object(object_fields) => {
                for object_field in object_fields {
                    object_field
                        .field_type
                        .validate_for_schemas(context.clone(), span, validation_state);
                }
            }
            Self::Variant { discriminator: _, cases } => {
                for variant_case in cases {
                    for object_field in &variant_case.fields {
                        object_field
                            .field_type
                            .validate_for_schemas(context.clone(), span, validation_state);
                    }
                }
            }
            Self::StringEnumReference(reference) => {
                reference.validate_type_expression_string_enum_reference(context, validation_state);
            }
            Self::String | Self::Number | Self::Float | Self::Boolean | Self::Null | Self::AnyObject | Self::StringEnum(_) => {}
        }
    }
}

impl Reference {
    fn validate_type_expression_string_enum_reference(&self, context: ValidationContext, validation_state: &mut SchemaValidationState) {
        if self.validate_schema_string_enum_type_reference(&context, validation_state) {
            return;
        }

        let Some(reference_root_keyword) = self.root_keyword() else {
            self.push_invalid_type_expression_reference(context, validation_state);

            return;
        };

        if !matches!(reference_root_keyword, ReferenceKeyword::Agent | ReferenceKeyword::Input) {
            self.push_invalid_type_expression_reference(context, validation_state);
        }
    }

    fn validate_schema_string_enum_type_reference(
        &self,
        context: &ValidationContext,
        validation_state: &mut SchemaValidationState,
    ) -> bool {
        let Some((schema_name, field_path)) = self.schema_name_and_field_path() else {
            return false;
        };

        if field_path.is_empty() {
            self.push_invalid_type_expression_reference(context.clone(), validation_state);

            return true;
        }

        let Some(schema_type_expression) = validation_state.validation_index.schema_type_expression(schema_name, self.span) else {
            validation_state.push_unknown_schema_reference(schema_name.to_string(), context.clone(), Some(self.span));

            return true;
        };

        let Some(field_type_expression) = schema_type_expression.field_type_at_path(&field_path) else {
            self.push_invalid_type_expression_reference(context.clone(), validation_state);

            return true;
        };

        let named_schema_types = validation_state.validation_index.named_schema_types(self.span);

        if field_type_expression.is_resolved_string_enum_expression(&named_schema_types) {
            return true;
        }

        self.push_invalid_type_expression_reference(context.clone(), validation_state);

        true
    }

    fn push_invalid_type_expression_reference(&self, context: ValidationContext, validation_state: &mut SchemaValidationState) {
        let reference_path = self.render_path();
        let issue_key = (context.clone(), reference_path.clone());

        if validation_state.invalid_type_expression_references.insert(issue_key) {
            validation_state.validation_report.push_issue_with_span(
                ValidationIssue::InvalidTypeExpressionReference { reference_path, context },
                Some(self.span),
            );
        }
    }
}
