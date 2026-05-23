use super::super::ast::{
    AgentDeclaration, AgentForLoop, AgentForLoopPattern, AgentProperty, Declaration, Expression, MatchBranch, ObjectField, Reference,
    ReferenceKeyword, SourceSpan, StringTemplatePart, TypeExpression, TypeExpressionFieldCache, Workflow,
};
use super::report::{ValidationContext, ValidationReport};
use super::tools::validate_agent_tool_bindings;
use crate::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::WorkflowSemanticIndex as ValidationIndex;
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_lines)]
pub(super) fn validate_agent_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut keyword_reference_validation_state = KeywordReferenceValidationState::new(workflow, validation_index, validation_report);
    let mut workflow_dynamic_field_types = keyword_reference_validation_state
        .for_loop_type_inference_context
        .local_binding_types
        .clone();
    let workflow_dynamic_fields = workflow
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            Declaration::Dynamic(dynamic_block) => Some(dynamic_block.fields.as_slice()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();

    keyword_reference_validation_state.infer_dynamic_field_types(workflow_dynamic_fields.as_slice(), &mut workflow_dynamic_field_types);

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                for provider_property in &provider_declaration.properties {
                    keyword_reference_validation_state.validate_expression(
                        &provider_property.value,
                        &workflow_dynamic_field_types,
                        provider_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::Model(model_declaration) => {
                let model_context = ValidationContext::Model(model_declaration.name.clone());

                for model_property in &model_declaration.properties {
                    keyword_reference_validation_state.validate_expression(
                        &model_property.value,
                        &workflow_dynamic_field_types,
                        model_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::Dynamic(dynamic_block) => {
                for dynamic_field in &dynamic_block.fields {
                    keyword_reference_validation_state.validate_expression(
                        &dynamic_field.value,
                        &workflow_dynamic_field_types,
                        ValidationContext::Dynamic,
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());
                let mut agent_dynamic_field_types = workflow_dynamic_field_types.clone();
                let agent_dynamic_fields = agent_declaration
                    .dynamic_blocks()
                    .flat_map(|dynamic_block| dynamic_block.fields.iter())
                    .collect::<Vec<_>>();

                if let Some(agent_for_loop) = &agent_declaration.for_loop {
                    keyword_reference_validation_state.validate_expression(
                        &agent_for_loop.iterable,
                        &agent_dynamic_field_types,
                        agent_context.clone(),
                        SecretReferencePolicy::Allow,
                    );

                    keyword_reference_validation_state.validate_for_loop_iterable_type(agent_declaration, agent_for_loop);

                    if let Some(iterable_item_type) = keyword_reference_validation_state.infer_for_loop_item_type(agent_for_loop) {
                        let binding_resolution = agent_for_loop.resolve_binding_types(&iterable_item_type);

                        for invalid_binding_name in binding_resolution.invalid_binding_names {
                            keyword_reference_validation_state.validation_report.push_issue_with_span(
                                agent_declaration.invalid_for_loop_destructuring_binding_issue(invalid_binding_name.as_str()),
                                Some(agent_declaration.span),
                            );
                        }

                        for (binding_name, binding_type) in binding_resolution.binding_types {
                            agent_dynamic_field_types.insert(binding_name, binding_type);
                        }
                    }
                }

                keyword_reference_validation_state
                    .infer_dynamic_field_types(agent_dynamic_fields.as_slice(), &mut agent_dynamic_field_types);

                for agent_property in &agent_declaration.properties {
                    match agent_property {
                        AgentProperty::InvalidModel(model_expression)
                        | AgentProperty::Instruction(model_expression)
                        | AgentProperty::Context(model_expression) => {
                            keyword_reference_validation_state.validate_expression(
                                model_expression,
                                &agent_dynamic_field_types,
                                agent_context.clone(),
                                SecretReferencePolicy::Forbid,
                            );
                        }
                        AgentProperty::Dynamic(dynamic_block) => {
                            for dynamic_field in &dynamic_block.fields {
                                keyword_reference_validation_state.validate_expression(
                                    &dynamic_field.value,
                                    &agent_dynamic_field_types,
                                    agent_context.clone(),
                                    SecretReferencePolicy::Allow,
                                );
                            }
                        }
                        AgentProperty::Model(model_usage) => {
                            for model_property in &model_usage.properties {
                                keyword_reference_validation_state.validate_expression(
                                    &model_property.value,
                                    &agent_dynamic_field_types,
                                    agent_context.clone(),
                                    SecretReferencePolicy::Allow,
                                );
                            }
                        }
                        AgentProperty::Uses(uses_expression) => {
                            keyword_reference_validation_state.validate_expression(
                                uses_expression,
                                &agent_dynamic_field_types,
                                agent_context.clone(),
                                SecretReferencePolicy::Allow,
                            );
                            validate_agent_tool_bindings(
                                agent_declaration,
                                uses_expression,
                                &agent_dynamic_field_types,
                                keyword_reference_validation_state.validation_index,
                                &keyword_reference_validation_state.for_loop_type_inference_context,
                                keyword_reference_validation_state.validation_report,
                            );
                        }
                        AgentProperty::Output { fields, span: _ } => {
                            for output_field in fields {
                                keyword_reference_validation_state.validate_type_expression_references(
                                    &output_field.field_type,
                                    &agent_dynamic_field_types,
                                    agent_context.clone(),
                                    SecretReferencePolicy::Forbid,
                                );
                            }
                        }
                        AgentProperty::Unknown { name: _, span: _ } => {}
                    }
                }
            }
            Declaration::Output(output_declaration) => {
                for output_field in &output_declaration.fields {
                    keyword_reference_validation_state.validate_expression(
                        &output_field.value,
                        &workflow_dynamic_field_types,
                        ValidationContext::Output,
                        SecretReferencePolicy::Forbid,
                    );
                }
            }
            Declaration::McpResource(resource_import_declaration) => {
                let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                for parameter in &resource_import_declaration.parameters {
                    keyword_reference_validation_state.validate_expression(
                        &parameter.value,
                        &workflow_dynamic_field_types,
                        resource_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::McpPrompt(prompt_import_declaration) => {
                let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                for parameter in &prompt_import_declaration.parameters {
                    keyword_reference_validation_state.validate_expression(
                        &parameter.value,
                        &workflow_dynamic_field_types,
                        prompt_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::McpBatch(batch_import_declaration) => {
                for tool_declaration in declaration.tool_declarations() {
                    let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                    for binding_field in &tool_declaration.fixed_binding_fields {
                        keyword_reference_validation_state.validate_expression(
                            &binding_field.value,
                            &workflow_dynamic_field_types,
                            tool_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }

                for resource_import_declaration in &batch_import_declaration.resources {
                    let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                    for parameter in &resource_import_declaration.parameters {
                        keyword_reference_validation_state.validate_expression(
                            &parameter.value,
                            &workflow_dynamic_field_types,
                            resource_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }

                for prompt_import_declaration in &batch_import_declaration.prompts {
                    let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                    for parameter in &prompt_import_declaration.parameters {
                        keyword_reference_validation_state.validate_expression(
                            &parameter.value,
                            &workflow_dynamic_field_types,
                            prompt_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }
            }
            Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                for resource_import_declaration in &resource_batch_import_declaration.resources {
                    let resource_context = ValidationContext::Resource(resource_import_declaration.name.clone());

                    for parameter in &resource_import_declaration.parameters {
                        keyword_reference_validation_state.validate_expression(
                            &parameter.value,
                            &workflow_dynamic_field_types,
                            resource_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }
            }
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                    let prompt_context = ValidationContext::Prompt(prompt_import_declaration.name.clone());

                    for parameter in &prompt_import_declaration.parameters {
                        keyword_reference_validation_state.validate_expression(
                            &parameter.value,
                            &workflow_dynamic_field_types,
                            prompt_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }
            }
            Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                for tool_declaration in declaration.tool_declarations() {
                    let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                    for binding_field in &tool_declaration.fixed_binding_fields {
                        keyword_reference_validation_state.validate_expression(
                            &binding_field.value,
                            &workflow_dynamic_field_types,
                            tool_context.clone(),
                            SecretReferencePolicy::Allow,
                        );
                    }
                }
            }
            Declaration::McpServer(_) | Declaration::Secrets(_) | Declaration::Input(_) | Declaration::Schema(_) => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretReferencePolicy {
    Allow,
    Forbid,
}

struct KeywordReferenceValidationState<'validation> {
    validation_index: &'validation ValidationIndex,
    validation_report: &'validation mut ValidationReport,
    for_loop_type_inference_context: TypeInferenceContext,
    unknown_agent_references: HashSet<(ValidationContext, String)>,
    invalid_keyword_reference_roots: HashSet<(ValidationContext, ReferenceKeyword)>,
    secret_reference_leaks: HashSet<(ValidationContext, String)>,
    missing_agent_output_type_references: HashSet<(ValidationContext, String)>,
    missing_optional_reference_accesses: HashSet<(ValidationContext, String, String)>,
    invalid_reference_paths: HashSet<(ValidationContext, String, String)>,
    missing_dynamic_declaration_contexts: HashSet<ValidationContext>,
    missing_input_declaration_contexts: HashSet<ValidationContext>,
    missing_secrets_declaration_contexts: HashSet<ValidationContext>,
    unknown_dynamic_field_references: HashSet<(ValidationContext, String)>,
    unknown_local_binding_references: HashSet<(ValidationContext, String)>,
    unknown_input_field_references: HashSet<(ValidationContext, String)>,
    unknown_secrets_field_references: HashSet<(ValidationContext, String)>,
    unknown_resource_references: HashSet<(ValidationContext, String)>,
    unknown_prompt_references: HashSet<(ValidationContext, String)>,
}

impl<'validation> KeywordReferenceValidationState<'validation> {
    fn new(
        workflow: &Workflow,
        validation_index: &'validation ValidationIndex,
        validation_report: &'validation mut ValidationReport,
    ) -> Self {
        let for_loop_type_inference_context = Self::build_for_loop_type_inference_context(workflow);

        Self {
            validation_index,
            validation_report,
            for_loop_type_inference_context,
            unknown_agent_references: HashSet::new(),
            invalid_keyword_reference_roots: HashSet::new(),
            secret_reference_leaks: HashSet::new(),
            missing_agent_output_type_references: HashSet::new(),
            missing_optional_reference_accesses: HashSet::new(),
            invalid_reference_paths: HashSet::new(),
            missing_dynamic_declaration_contexts: HashSet::new(),
            missing_input_declaration_contexts: HashSet::new(),
            missing_secrets_declaration_contexts: HashSet::new(),
            unknown_dynamic_field_references: HashSet::new(),
            unknown_local_binding_references: HashSet::new(),
            unknown_input_field_references: HashSet::new(),
            unknown_secrets_field_references: HashSet::new(),
            unknown_resource_references: HashSet::new(),
            unknown_prompt_references: HashSet::new(),
        }
    }

    fn build_for_loop_type_inference_context(workflow: &Workflow) -> TypeInferenceContext {
        let named_schema_types = workflow.named_schema_types();

        let input_type = workflow.find_input().and_then(|input_declaration| {
            TypeExpression::Object(input_declaration.fields.clone())
                .to_workflow_type(&named_schema_types)
                .ok()
        });

        let secrets_type = workflow.find_secrets().and_then(|secrets_declaration| {
            TypeExpression::Object(secrets_declaration.fields.clone())
                .to_workflow_type(&named_schema_types)
                .ok()
        });

        let mut agent_output_types = HashMap::new();
        let mut tool_input_types = HashMap::new();
        let mut tool_binding_types = HashMap::new();
        let mut tool_output_types = HashMap::new();

        for tool_declaration in workflow.tool_declarations() {
            if let Ok(tool_input_type) = TypeExpression::Object(tool_declaration.input_fields.clone()).to_workflow_type(&named_schema_types)
            {
                tool_input_types.insert(tool_declaration.name.clone(), tool_input_type);
            }

            if let Ok(tool_binding_type) =
                TypeExpression::Object(tool_declaration.binding_fields.clone()).to_workflow_type(&named_schema_types)
            {
                tool_binding_types.insert(tool_declaration.name.clone(), tool_binding_type);
            }

            if tool_declaration.has_untyped_mcp_output() {
                tool_output_types.insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
            } else if let Ok(tool_output_type) =
                TypeExpression::Object(tool_declaration.output_fields.clone()).to_workflow_type(&named_schema_types)
            {
                tool_output_types.insert(tool_declaration.name.clone(), tool_output_type);
            }
        }

        for declaration in workflow.declarations() {
            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let final_output_type_expression = agent_declaration.inferred_final_output_type_expression();
            let inferred_output_type = final_output_type_expression.to_workflow_type(&named_schema_types);

            let Ok(inferred_output_type) = inferred_output_type else {
                continue;
            };

            agent_output_types.insert(agent_declaration.name.clone(), inferred_output_type);
        }

        let mut local_binding_types = HashMap::new();

        let mut type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            tool_input_types,
            tool_binding_types,
            tool_output_types,
            local_binding_types: HashMap::new(),
        };

        for dynamic_block in workflow.dynamic_blocks() {
            for dynamic_field in &dynamic_block.fields {
                let Ok(dynamic_field_type) = infer_expression_type(
                    &dynamic_field.value,
                    &type_inference_context,
                    &format!("dynamic field `{}` type inference", dynamic_field.name),
                ) else {
                    continue;
                };

                local_binding_types.insert(dynamic_field.name.clone(), dynamic_field_type.clone());
                type_inference_context
                    .local_binding_types
                    .insert(dynamic_field.name.clone(), dynamic_field_type);
            }
        }

        type_inference_context
    }

    fn validate_for_loop_iterable_type(&mut self, agent_declaration: &AgentDeclaration, agent_for_loop: &AgentForLoop) {
        let type_inference_context = &self.for_loop_type_inference_context;
        let inferred_iterable_type = infer_expression_type(
            &agent_for_loop.iterable,
            type_inference_context,
            &format!("for-loop iterable for agent `{}`", agent_declaration.name),
        );

        let Ok(inferred_iterable_type) = inferred_iterable_type else {
            return;
        };

        if inferred_iterable_type.is_guaranteed_array() {
            return;
        }

        self.validation_report.push_issue_with_span(
            agent_declaration.invalid_for_loop_iterable_type_issue(inferred_iterable_type.to_string()),
            Some(agent_declaration.span),
        );
    }

    fn infer_for_loop_item_type(&self, agent_for_loop: &AgentForLoop) -> Option<WorkflowType> {
        let inferred_iterable_type = infer_expression_type(
            &agent_for_loop.iterable,
            &self.for_loop_type_inference_context,
            "for-loop iterable item inference",
        )
        .ok()?;

        match inferred_iterable_type {
            WorkflowType::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            WorkflowType::Union(union_members) => union_members.into_iter().find_map(|union_member| match union_member {
                WorkflowType::Array {
                    item_type,
                    fixed_length: _,
                } => Some(*item_type),
                WorkflowType::Any
                | WorkflowType::String
                | WorkflowType::Integer
                | WorkflowType::Float
                | WorkflowType::Boolean
                | WorkflowType::Null
                | WorkflowType::AnyObject
                | WorkflowType::StringEnum(_)
                | WorkflowType::Union(_)
                | WorkflowType::Tuple(_)
                | WorkflowType::Object(_)
                | WorkflowType::Variant {
                    discriminator: _,
                    cases: _,
                } => None,
            }),
            WorkflowType::Any
            | WorkflowType::String
            | WorkflowType::Integer
            | WorkflowType::Float
            | WorkflowType::Boolean
            | WorkflowType::Null
            | WorkflowType::AnyObject
            | WorkflowType::StringEnum(_)
            | WorkflowType::Tuple(_)
            | WorkflowType::Object(_)
            | WorkflowType::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    fn infer_dynamic_field_types(&self, dynamic_fields: &[&ObjectField], dynamic_field_types: &mut HashMap<String, WorkflowType>) {
        let mut pending_dynamic_fields = dynamic_fields.to_vec();

        while !pending_dynamic_fields.is_empty() {
            let pending_count_before_pass = pending_dynamic_fields.len();

            pending_dynamic_fields.retain(|dynamic_field| {
                let mut type_inference_context = self.for_loop_type_inference_context.clone();
                type_inference_context.local_binding_types.clone_from(dynamic_field_types);

                let Ok(dynamic_field_type) =
                    infer_expression_type(&dynamic_field.value, &type_inference_context, dynamic_field.name.as_str())
                else {
                    return true;
                };

                dynamic_field_types.insert(dynamic_field.name.clone(), dynamic_field_type);

                false
            });

            if pending_dynamic_fields.len() == pending_count_before_pass {
                break;
            }
        }
    }

    fn validate_expression(
        &mut self,
        expression: &Expression,
        dynamic_field_types: &HashMap<String, WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        match expression {
            Expression::Reference(reference) => {
                self.validate_reference(reference, dynamic_field_types, context, secret_reference_policy);
            }
            Expression::FunctionCall(function_call) => {
                self.validate_reference(&function_call.callee, dynamic_field_types, context.clone(), secret_reference_policy);

                for call_argument in &function_call.arguments {
                    self.validate_expression(
                        call_argument.expression(),
                        dynamic_field_types,
                        context.clone(),
                        secret_reference_policy,
                    );
                }
            }
            Expression::ToolCall(tool_call) => {
                self.validate_reference(&tool_call.callee, dynamic_field_types, context.clone(), secret_reference_policy);

                for object_field in &tool_call.input_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }

                for object_field in &tool_call.binding_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::McpCall(mcp_call) => {
                self.validate_mcp_call(mcp_call, dynamic_field_types, context, secret_reference_policy);
            }
            Expression::NullFallback(null_fallback) => {
                self.validate_expression(&null_fallback.value, dynamic_field_types, context.clone(), secret_reference_policy);
                self.validate_expression(&null_fallback.fallback, dynamic_field_types, context, secret_reference_policy);
            }
            Expression::VariantProjection(variant_projection) => {
                self.validate_reference(&variant_projection.value, dynamic_field_types, context, secret_reference_policy);
            }
            Expression::Match(match_expression) => {
                self.validate_expression(
                    &match_expression.value,
                    dynamic_field_types,
                    context.clone(),
                    secret_reference_policy,
                );

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        self.validate_expression(value, dynamic_field_types, context.clone(), secret_reference_policy);
                    }
                }
            }
            Expression::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    self.validate_expression(array_value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        self.validate_expression(
                            interpolation_expression,
                            dynamic_field_types,
                            context.clone(),
                            secret_reference_policy,
                        );
                    }
                }
            }
            Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
        }
    }

    fn validate_type_expression_references(
        &mut self,
        type_expression: &TypeExpression,
        dynamic_field_types: &HashMap<String, WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        match type_expression {
            TypeExpression::StringEnumReference(reference) if reference.schema_name_and_field_path().is_none() => {
                self.validate_reference(reference, dynamic_field_types, context, secret_reference_policy);
            }
            TypeExpression::Array {
                item_type,
                fixed_length: _,
            } => {
                self.validate_type_expression_references(item_type, dynamic_field_types, context, secret_reference_policy);
            }
            TypeExpression::Tuple(type_expressions) | TypeExpression::Union(type_expressions) => {
                for nested_type_expression in type_expressions {
                    self.validate_type_expression_references(
                        nested_type_expression,
                        dynamic_field_types,
                        context.clone(),
                        secret_reference_policy,
                    );
                }
            }
            TypeExpression::Object(fields) => {
                for field in fields {
                    self.validate_type_expression_references(
                        &field.field_type,
                        dynamic_field_types,
                        context.clone(),
                        secret_reference_policy,
                    );
                }
            }
            TypeExpression::Variant { discriminator: _, cases } => {
                for variant_case in cases {
                    for field in &variant_case.fields {
                        self.validate_type_expression_references(
                            &field.field_type,
                            dynamic_field_types,
                            context.clone(),
                            secret_reference_policy,
                        );
                    }
                }
            }
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_) => {}
        }
    }

    fn validate_mcp_call(
        &mut self,
        mcp_call: &crate::dsl::McpCall,
        dynamic_field_types: &HashMap<String, WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        if !mcp_call.has_valid_callee() {
            let issue_key = (context.clone(), mcp_call.operation.expected_root());

            if self.invalid_keyword_reference_roots.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::invalid_keyword_root_issue(mcp_call.operation.expected_root(), context.clone()),
                    Some(mcp_call.callee.span),
                );
            }
        } else if let Some(target_name) = mcp_call.target_name() {
            match mcp_call.operation {
                crate::dsl::McpCallOperation::Read => {
                    self.validate_resource_call_name(target_name, context.clone(), &mcp_call.callee);
                }
                crate::dsl::McpCallOperation::Render => {
                    self.validate_prompt_call_name(target_name, context.clone(), &mcp_call.callee);
                }
            }
        }

        for parameter_field in &mcp_call.parameter_fields {
            self.validate_expression(
                &parameter_field.value,
                dynamic_field_types,
                context.clone(),
                secret_reference_policy,
            );
        }
    }

    fn validate_resource_call_name(&mut self, resource_name: &str, context: ValidationContext, reference: &Reference) {
        if self.validation_index.has_resource(resource_name) {
            return;
        }

        let issue_key = (context.clone(), resource_name.to_string());

        if self.unknown_resource_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                Reference::unknown_resource_reference_issue(resource_name, context),
                Some(reference.span),
            );
        }
    }

    fn validate_prompt_call_name(&mut self, prompt_name: &str, context: ValidationContext, reference: &Reference) {
        if self.validation_index.has_prompt(prompt_name) {
            return;
        }

        let issue_key = (context.clone(), prompt_name.to_string());

        if self.unknown_prompt_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                Reference::unknown_prompt_reference_issue(prompt_name, context),
                Some(reference.span),
            );
        }
    }

    fn validate_reference(
        &mut self,
        reference: &Reference,
        dynamic_field_types: &HashMap<String, crate::semantic::support::types::WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        let Some(reference_root_keyword) = reference.root_keyword() else {
            self.validate_local_binding_reference(reference, dynamic_field_types, context);

            return;
        };

        if reference.is_secret_reference() && secret_reference_policy == SecretReferencePolicy::Forbid {
            self.push_secret_reference_leak(reference, context.clone());
        }

        if !reference.has_accesses() {
            let issue_key = (context.clone(), reference_root_keyword);

            if self.invalid_keyword_reference_roots.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::invalid_keyword_root_issue(reference_root_keyword, context),
                    Some(reference.span),
                );
            }

            return;
        }

        match reference_root_keyword {
            ReferenceKeyword::Agent => {
                self.validate_agent_reference(reference, context);
            }
            ReferenceKeyword::Dynamic => {
                self.validate_dynamic_reference(reference, dynamic_field_types, context);
            }
            ReferenceKeyword::Input => {
                self.validate_input_reference(reference, context);
            }
            ReferenceKeyword::Secrets => {
                self.validate_secrets_reference(reference, context);
            }
            ReferenceKeyword::Model | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt => {}
        }
    }

    fn validate_local_binding_reference(
        &mut self,
        reference: &Reference,
        local_binding_types: &HashMap<String, WorkflowType>,
        context: ValidationContext,
    ) {
        let Some(binding_name) = reference.root_identifier() else {
            return;
        };

        let Some(local_binding_type) = local_binding_types.get(binding_name) else {
            let issue_key = (context.clone(), binding_name.to_owned());

            if self.unknown_local_binding_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::unknown_local_binding_reference_issue(binding_name, context),
                    Some(reference.span),
                );
            }

            return;
        };

        self.validate_workflow_type_reference_path_from(reference, local_binding_type.clone(), context, 0);
    }

    fn validate_agent_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_agent_name = reference
            .first_access()
            .expect("agent reference should include first field after root")
            .field
            .as_str();

        if !self.validate_agent_reference_name(referenced_agent_name, context.clone(), reference) {
            return;
        }

        let referenced_agent_output_type = self
            .validation_index
            .agent_output_type(referenced_agent_name)
            .and_then(Clone::clone);

        if reference.has_single_access() {
            if context == ValidationContext::Output && referenced_agent_output_type.is_none() {
                self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference);
            }

            return;
        }

        let Some(agent_output_type) = referenced_agent_output_type else {
            self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference);

            return;
        };

        self.validate_reference_path(reference, agent_output_type, context);
    }

    fn push_missing_agent_output_type_reference_issue(
        &mut self,
        referenced_agent_name: &str,
        context: ValidationContext,
        reference: &Reference,
    ) {
        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.missing_agent_output_type_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                Reference::missing_agent_output_type_reference_issue(referenced_agent_name, context),
                Some(reference.span),
            );
        }
    }

    fn validate_dynamic_reference(
        &mut self,
        reference: &Reference,
        dynamic_field_types: &HashMap<String, WorkflowType>,
        context: ValidationContext,
    ) {
        let referenced_field_name = reference
            .first_access()
            .expect("dynamic reference should include first field after root")
            .field
            .as_str();

        let Some(dynamic_field_type) = dynamic_field_types.get(referenced_field_name) else {
            if dynamic_field_types.is_empty() {
                if self.missing_dynamic_declaration_contexts.insert(context.clone()) {
                    self.validation_report
                        .push_issue_with_span(Reference::missing_dynamic_declaration_issue(context), Some(reference.span));
                }

                return;
            }

            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_dynamic_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::unknown_dynamic_field_reference_issue(referenced_field_name, context),
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.has_single_access() {
            return;
        }

        self.validate_workflow_type_reference_path(reference, dynamic_field_type.clone(), context);
    }

    fn validate_input_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .first_access()
            .expect("input reference should include first field after root")
            .field
            .as_str();

        let Some(input_field_types) = self.validation_index.input_field_types() else {
            if self.missing_input_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(Reference::missing_input_declaration_issue(context), Some(reference.span));
            }

            return;
        };

        let Some(input_field_type) = input_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_input_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::unknown_input_field_reference_issue(referenced_field_name, context),
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.has_single_access() {
            return;
        }

        self.validate_reference_path(reference, input_field_type.clone(), context);
    }

    fn validate_secrets_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .first_access()
            .expect("secrets reference should include first field after root")
            .field
            .as_str();

        let Some(secrets_field_types) = self.validation_index.secrets_field_types() else {
            if self.missing_secrets_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(Reference::missing_secrets_declaration_issue(context), Some(reference.span));
            }

            return;
        };

        let Some(secrets_field_type) = secrets_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_secrets_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    Reference::unknown_secrets_field_reference_issue(referenced_field_name, context),
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.has_single_access() {
            return;
        }

        self.validate_reference_path(reference, secrets_field_type.clone(), context);
    }

    fn validate_reference_path(&mut self, reference: &Reference, start_type: TypeExpression, context: ValidationContext) {
        let mut candidate_types = vec![start_type];
        let mut field_cache = TypeExpressionFieldCache::new();

        for reference_access in reference.projection_accesses() {
            if candidate_types.iter().any(TypeExpression::can_be_null) && !reference_access.optional {
                self.push_missing_optional_reference_access(reference, reference_access.field.as_str(), context.clone());

                return;
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                candidate_type.collect_field_types_for_access_with_cache(
                    reference_access.field.as_str(),
                    &mut |schema_name| self.validation_index.schema_type_expression(schema_name, SourceSpan::generated()),
                    &mut next_candidate_types,
                    &mut field_cache,
                );
            }

            if reference_access.optional {
                next_candidate_types.push(TypeExpression::Null);
            }

            if next_candidate_types.is_empty() {
                let reference_path = reference.render_path();
                let issue_key = (context.clone(), reference_path.clone(), reference_access.field.clone());

                if self.invalid_reference_paths.insert(issue_key) {
                    self.validation_report
                        .push_issue_with_span(reference.invalid_path_issue(reference_access, context), Some(reference.span));
                }

                return;
            }

            candidate_types = next_candidate_types;
        }
    }

    fn validate_workflow_type_reference_path(&mut self, reference: &Reference, start_type: WorkflowType, context: ValidationContext) {
        self.validate_workflow_type_reference_path_from(reference, start_type, context, 1);
    }

    fn validate_workflow_type_reference_path_from(
        &mut self,
        reference: &Reference,
        start_type: WorkflowType,
        context: ValidationContext,
        access_start_index: usize,
    ) {
        let mut candidate_types = vec![start_type];

        for reference_access in reference.accesses_from(access_start_index) {
            if candidate_types.iter().any(workflow_type_can_be_null) && !reference_access.optional {
                self.push_missing_optional_reference_access(reference, reference_access.field.as_str(), context.clone());

                return;
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                Self::collect_next_workflow_types_for_field(candidate_type, reference_access.field.as_str(), &mut next_candidate_types);
            }

            if reference_access.optional {
                next_candidate_types.push(crate::semantic::support::types::WorkflowType::Null);
            }

            if next_candidate_types.is_empty() {
                let reference_path = reference.render_path();
                let issue_key = (context.clone(), reference_path.clone(), reference_access.field.clone());

                if self.invalid_reference_paths.insert(issue_key) {
                    self.validation_report
                        .push_issue_with_span(reference.invalid_path_issue(reference_access, context), Some(reference.span));
                }

                return;
            }

            candidate_types = next_candidate_types;
        }
    }

    fn push_missing_optional_reference_access(&mut self, reference: &Reference, field_name: &str, context: ValidationContext) {
        let reference_path = reference.render_path();
        let issue_key = (context.clone(), reference_path.clone(), field_name.to_owned());

        if self.missing_optional_reference_accesses.insert(issue_key) {
            self.validation_report
                .push_issue_with_span(reference.missing_optional_access_issue(field_name, context), Some(reference.span));
        }
    }

    fn collect_next_workflow_types_for_field(
        candidate_type: &WorkflowType,
        field_name: &str,
        next_candidate_types: &mut Vec<WorkflowType>,
    ) {
        match candidate_type {
            WorkflowType::Object(fields) => {
                if let Some(field_type) = fields.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            WorkflowType::Union(union_members) => {
                for union_member in union_members {
                    Self::collect_next_workflow_types_for_field(union_member, field_name, next_candidate_types);
                }
            }
            WorkflowType::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    next_candidate_types.extend(cases.keys().cloned().map(|case_name| WorkflowType::StringEnum(vec![case_name])));
                }
            }
            WorkflowType::Any
            | WorkflowType::String
            | WorkflowType::Integer
            | WorkflowType::Float
            | WorkflowType::Boolean
            | WorkflowType::Null
            | WorkflowType::AnyObject
            | WorkflowType::StringEnum(_)
            | WorkflowType::Array {
                item_type: _,
                fixed_length: _,
            }
            | WorkflowType::Tuple(_) => {}
        }
    }

    fn push_secret_reference_leak(&mut self, reference: &Reference, context: ValidationContext) {
        let reference_path = reference.render_path();
        let issue_key = (context.clone(), reference_path.clone());

        if self.secret_reference_leaks.insert(issue_key) {
            self.validation_report
                .push_issue_with_span(reference.secret_reference_in_llm_context_issue(context), Some(reference.span));
        }
    }

    fn validate_agent_reference_name(&mut self, referenced_agent_name: &str, context: ValidationContext, reference: &Reference) -> bool {
        if self.validation_index.has_agent(referenced_agent_name) {
            return true;
        }

        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.unknown_agent_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                Reference::unknown_agent_reference_issue(referenced_agent_name, context),
                Some(reference.span),
            );
        }

        false
    }
}

struct ForLoopBindingResolution {
    binding_types: HashMap<String, WorkflowType>,
    invalid_binding_names: Vec<String>,
}

impl AgentForLoop {
    fn resolve_binding_types(&self, iterable_item_type: &WorkflowType) -> ForLoopBindingResolution {
        let mut binding_types = HashMap::new();
        let mut invalid_binding_names = Vec::new();

        match &self.pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                binding_types.insert(identifier.clone(), iterable_item_type.clone());
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                for field_name in field_names {
                    let Some(field_type) = iterable_item_type.field_type(field_name) else {
                        invalid_binding_names.push(field_name.clone());
                        binding_types.insert(field_name.clone(), WorkflowType::Any);

                        continue;
                    };

                    binding_types.insert(field_name.clone(), field_type);
                }
            }
        }

        ForLoopBindingResolution {
            binding_types,
            invalid_binding_names,
        }
    }
}

fn workflow_type_can_be_null(workflow_type: &WorkflowType) -> bool {
    match workflow_type {
        WorkflowType::Null => true,
        WorkflowType::Union(union_members) => union_members.iter().any(workflow_type_can_be_null),
        WorkflowType::Any
        | WorkflowType::String
        | WorkflowType::Integer
        | WorkflowType::Float
        | WorkflowType::Boolean
        | WorkflowType::AnyObject
        | WorkflowType::StringEnum(_)
        | WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | WorkflowType::Tuple(_)
        | WorkflowType::Object(_)
        | WorkflowType::Variant {
            discriminator: _,
            cases: _,
        } => false,
    }
}
