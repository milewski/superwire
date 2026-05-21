use super::super::ast::{
    AgentDeclaration, AgentForLoop, AgentProperty, Declaration, Expression, FunctionCall, MatchBranch, ObjectField, Reference,
    ReferenceKeyword, SourcePosition, SourceSpan, StringTemplatePart, ToolCall, TypeExpression, Workflow,
};
use super::collect_agent_dependencies_from_expression;
use super::index::ValidationIndex;
use super::report::{ValidationContext, ValidationIssue, ValidationReport};
use crate::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::semantic::support::types::{ensure_type_matches, workflow_type_from_dsl, WorkflowType};
use std::collections::{HashMap, HashSet};

pub(super) trait ToolReferenceCollector {
    fn referenced_names_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Vec<String>;
}

impl ToolReferenceCollector for Expression {
    fn referenced_names_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Vec<String> {
        let Expression::ArrayLiteral(tool_expressions) = self else {
            return Vec::new();
        };

        tool_expressions
            .iter()
            .filter_map(|expression| expression.direct_name_for_keyword(reference_keyword))
            .collect()
    }
}

trait DirectToolName {
    fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String>;
}

impl DirectToolName for Expression {
    fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        match self {
            Self::Reference(reference) => reference.direct_name_for_keyword(reference_keyword),
            Self::FunctionCall(function_call) => function_call.direct_name_for_keyword(reference_keyword),
            Self::ToolCall(tool_call) => tool_call.callee.direct_name_for_keyword(reference_keyword),
            Self::McpCall(_) => None,
            Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }
}

impl DirectToolName for FunctionCall {
    fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        self.callee.direct_name_for_keyword(reference_keyword)
    }
}

impl DirectToolName for Reference {
    fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        if self.root_keyword() != Some(reference_keyword) || self.accesses.len() != 1 || self.accesses[0].optional {
            return None;
        }

        Some(self.accesses[0].field.clone())
    }
}

trait AgentToolBindingFields {
    fn agent_tool_binding_fields(&self) -> &[ObjectField];
}

trait LiteralTypeCompatibility {
    fn is_literal_compatible_with_type(&self, expected_type: &WorkflowType) -> bool;
}

impl AgentToolBindingFields for Expression {
    fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        match self {
            Self::ToolCall(tool_call) => tool_call.agent_tool_binding_fields(),
            Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => &[],
        }
    }
}

impl AgentToolBindingFields for ToolCall {
    fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        self.binding_fields.as_slice()
    }
}

impl LiteralTypeCompatibility for Expression {
    fn is_literal_compatible_with_type(&self, expected_type: &WorkflowType) -> bool {
        match (self, expected_type) {
            (Self::StringLiteral(string_literal), WorkflowType::StringEnum(enum_values)) => enum_values.contains(string_literal),
            (Self::StringLiteral(_), WorkflowType::String) => true,
            (Self::NumberLiteral(number_literal), WorkflowType::Float) => number_literal.replace('_', "").contains('.'),
            (Self::NumberLiteral(number_literal), WorkflowType::Integer) => !number_literal.replace('_', "").contains('.'),
            (Self::BooleanLiteral(_), WorkflowType::Boolean) | (Self::NullLiteral, WorkflowType::Null) => true,
            (expression, WorkflowType::Union(union_members)) => union_members
                .iter()
                .any(|union_member| expression.is_literal_compatible_with_type(union_member)),
            _ => false,
        }
    }
}

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
                        for bound_identifier_name in agent_for_loop.bound_identifier_names() {
                            agent_dynamic_field_types.insert(bound_identifier_name.to_string(), iterable_item_type.clone());
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
                            keyword_reference_validation_state.validate_agent_tool_bindings(
                                agent_declaration,
                                uses_expression,
                                &agent_dynamic_field_types,
                            );
                        }
                        AgentProperty::Output { fields: _, span: _ } | AgentProperty::Unknown { name: _, span: _ } => {}
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
            unknown_input_field_references: HashSet::new(),
            unknown_secrets_field_references: HashSet::new(),
            unknown_resource_references: HashSet::new(),
            unknown_prompt_references: HashSet::new(),
        }
    }

    fn build_for_loop_type_inference_context(workflow: &Workflow) -> TypeInferenceContext {
        let mut named_schema_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Schema(schema_declaration) = declaration else {
                continue;
            };

            named_schema_types.insert(schema_declaration.name.clone(), schema_declaration.type_expression());
        }

        let input_type = workflow.find_input().and_then(|input_declaration| {
            workflow_type_from_dsl(&TypeExpression::Object(input_declaration.fields.clone()), &named_schema_types).ok()
        });

        let secrets_type = workflow.find_secrets().and_then(|secrets_declaration| {
            workflow_type_from_dsl(&TypeExpression::Object(secrets_declaration.fields.clone()), &named_schema_types).ok()
        });

        let mut agent_output_types = HashMap::new();
        let mut tool_input_types = HashMap::new();
        let mut tool_binding_types = HashMap::new();
        let mut tool_output_types = HashMap::new();

        for tool_declaration in workflow.tool_declarations() {
            if let Ok(tool_input_type) =
                workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
            {
                tool_input_types.insert(tool_declaration.name.clone(), tool_input_type);
            }

            if let Ok(tool_binding_type) = workflow_type_from_dsl(
                &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                &named_schema_types,
            ) {
                tool_binding_types.insert(tool_declaration.name.clone(), tool_binding_type);
            }

            if tool_declaration.has_untyped_mcp_output() {
                tool_output_types.insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
            } else if let Ok(tool_output_type) =
                workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
            {
                tool_output_types.insert(tool_declaration.name.clone(), tool_output_type);
            }
        }

        for declaration in workflow.declarations() {
            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let final_output_type_expression = agent_declaration.inferred_final_output_type_expression();
            let inferred_output_type = workflow_type_from_dsl(&final_output_type_expression, &named_schema_types);

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
            ValidationIssue::InvalidForLoopIterableType {
                agent_name: agent_declaration.name.clone(),
                found_type: inferred_iterable_type.to_string(),
            },
            Some(agent_declaration.span),
        );
    }

    fn validate_agent_tool_bindings(
        &mut self,
        agent_declaration: &AgentDeclaration,
        tools_expression: &Expression,
        local_binding_types: &HashMap<String, WorkflowType>,
    ) {
        let Expression::ArrayLiteral(tool_expressions) = tools_expression else {
            return;
        };

        for tool_expression in tool_expressions {
            let Some(tool_name) = tool_expression.direct_name_for_keyword(ReferenceKeyword::Tool) else {
                continue;
            };

            let Some(WorkflowType::Object(expected_binding_fields)) = self.validation_index.tool_binding_types.get(&tool_name) else {
                continue;
            };

            let binding_fields = tool_expression.agent_tool_binding_fields();
            self.validate_agent_tool_binding_fields(
                agent_declaration,
                &tool_name,
                binding_fields,
                expected_binding_fields,
                local_binding_types,
            );
        }
    }

    fn validate_agent_tool_binding_fields(
        &mut self,
        agent_declaration: &AgentDeclaration,
        tool_name: &str,
        binding_fields: &[ObjectField],
        expected_binding_fields: &std::collections::BTreeMap<String, WorkflowType>,
        local_binding_types: &HashMap<String, WorkflowType>,
    ) {
        if let Some(fixed_names) = self.validation_index.tool_fixed_binding_names.get(tool_name) {
            for binding_field in binding_fields {
                if fixed_names.contains(&binding_field.name) {
                    self.push_invalid_tool_binding(
                        agent_declaration,
                        tool_name,
                        format!(
                            "bound argument `{}` is already fixed in the tool declaration and cannot be overridden",
                            binding_field.name
                        ),
                        Some(binding_field.span),
                    );
                }
            }
        }

        self.validate_agent_tool_binding_self_references(agent_declaration, tool_name, binding_fields);

        for expected_binding_name in expected_binding_fields.keys() {
            if binding_fields
                .iter()
                .any(|binding_field| &binding_field.name == expected_binding_name)
            {
                continue;
            }

            self.push_invalid_tool_binding(
                agent_declaration,
                tool_name,
                format!("missing required bound argument `{expected_binding_name}`"),
                Some(agent_declaration.span),
            );
        }

        let mut type_inference_context = self.for_loop_type_inference_context.clone();
        type_inference_context.local_binding_types.clone_from(local_binding_types);

        for binding_field in binding_fields {
            let Some(expected_binding_type) = expected_binding_fields.get(&binding_field.name) else {
                self.push_invalid_tool_binding(
                    agent_declaration,
                    tool_name,
                    format!("unknown bound argument `{}`", binding_field.name),
                    Some(agent_declaration.span),
                );

                continue;
            };

            if binding_field.value.is_literal_compatible_with_type(expected_binding_type) {
                continue;
            }

            let Ok(actual_binding_type) = infer_expression_type(
                &binding_field.value,
                &type_inference_context,
                &format!("tool `tool.{tool_name}` bound argument `{}`", binding_field.name),
            ) else {
                continue;
            };

            if ensure_type_matches(expected_binding_type, &actual_binding_type) {
                continue;
            }

            self.push_invalid_tool_binding(
                agent_declaration,
                tool_name,
                format!(
                    "bound argument `{}` expects {}, found {}",
                    binding_field.name, expected_binding_type, actual_binding_type
                ),
                Some(agent_declaration.span),
            );
        }
    }

    fn validate_agent_tool_binding_self_references(
        &mut self,
        agent_declaration: &AgentDeclaration,
        tool_name: &str,
        binding_fields: &[ObjectField],
    ) {
        for binding_field in binding_fields {
            self.validate_agent_tool_binding_expression_self_reference(
                agent_declaration,
                tool_name,
                &binding_field.name,
                "agent tool binding override",
                &binding_field.value,
                binding_field.span,
            );
        }

        let Some(fixed_binding_fields) = self.validation_index.tool_fixed_binding_fields.get(tool_name).cloned() else {
            return;
        };

        for fixed_binding_field in &fixed_binding_fields {
            self.validate_agent_tool_binding_expression_self_reference(
                agent_declaration,
                tool_name,
                &fixed_binding_field.name,
                "tool declaration binding",
                &fixed_binding_field.value,
                fixed_binding_field.span,
            );
        }
    }

    fn validate_agent_tool_binding_expression_self_reference(
        &mut self,
        agent_declaration: &AgentDeclaration,
        tool_name: &str,
        binding_name: &str,
        binding_source: &str,
        expression: &Expression,
        span: SourceSpan,
    ) {
        let mut referenced_agents = HashSet::new();

        collect_agent_dependencies_from_expression(expression, &mut referenced_agents);

        if !referenced_agents.contains(&agent_declaration.name) {
            return;
        }

        self.push_invalid_tool_binding(
            agent_declaration,
            tool_name,
            format!(
                "{binding_source} `{binding_name}` references `agent.{}` while `tool.{tool_name}` is attached to agent `{}`; \
                 an agent cannot call a tool that requires its own output because that output is only available after the agent finishes. \
                 Move `tool.{tool_name}` to a later agent that depends on `{}`, or bind `{binding_name}` from input, dynamic data, or a previous agent",
                agent_declaration.name, agent_declaration.name, agent_declaration.name
            ),
            Some(span),
        );
    }

    fn push_invalid_tool_binding(
        &mut self,
        agent_declaration: &AgentDeclaration,
        tool_name: &str,
        message: String,
        span: Option<SourceSpan>,
    ) {
        self.validation_report.push_issue_with_span(
            ValidationIssue::InvalidToolBinding {
                agent_name: agent_declaration.name.clone(),
                tool_name: tool_name.to_string(),
                message,
            },
            span,
        );
    }

    fn infer_for_loop_item_type(&self, agent_for_loop: &AgentForLoop) -> Option<crate::semantic::support::types::WorkflowType> {
        let inferred_iterable_type = infer_expression_type(
            &agent_for_loop.iterable,
            &self.for_loop_type_inference_context,
            "for-loop iterable item inference",
        )
        .ok()?;

        match inferred_iterable_type {
            crate::semantic::support::types::WorkflowType::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            crate::semantic::support::types::WorkflowType::Union(union_members) => {
                union_members.into_iter().find_map(|union_member| match union_member {
                    crate::semantic::support::types::WorkflowType::Array {
                        item_type,
                        fixed_length: _,
                    } => Some(*item_type),
                    crate::semantic::support::types::WorkflowType::Any
                    | crate::semantic::support::types::WorkflowType::String
                    | crate::semantic::support::types::WorkflowType::Integer
                    | crate::semantic::support::types::WorkflowType::Float
                    | crate::semantic::support::types::WorkflowType::Boolean
                    | crate::semantic::support::types::WorkflowType::Null
                    | crate::semantic::support::types::WorkflowType::AnyObject
                    | crate::semantic::support::types::WorkflowType::StringEnum(_)
                    | crate::semantic::support::types::WorkflowType::Union(_)
                    | crate::semantic::support::types::WorkflowType::Tuple(_)
                    | crate::semantic::support::types::WorkflowType::Object(_)
                    | crate::semantic::support::types::WorkflowType::Variant {
                        discriminator: _,
                        cases: _,
                    } => None,
                })
            }
            crate::semantic::support::types::WorkflowType::Any
            | crate::semantic::support::types::WorkflowType::String
            | crate::semantic::support::types::WorkflowType::Integer
            | crate::semantic::support::types::WorkflowType::Float
            | crate::semantic::support::types::WorkflowType::Boolean
            | crate::semantic::support::types::WorkflowType::Null
            | crate::semantic::support::types::WorkflowType::AnyObject
            | crate::semantic::support::types::WorkflowType::StringEnum(_)
            | crate::semantic::support::types::WorkflowType::Tuple(_)
            | crate::semantic::support::types::WorkflowType::Object(_)
            | crate::semantic::support::types::WorkflowType::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    fn infer_dynamic_field_types(
        &self,
        dynamic_fields: &[&ObjectField],
        dynamic_field_types: &mut HashMap<String, crate::semantic::support::types::WorkflowType>,
    ) {
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
        dynamic_field_types: &HashMap<String, crate::semantic::support::types::WorkflowType>,
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

    fn validate_mcp_call(
        &mut self,
        mcp_call: &crate::dsl::McpCall,
        dynamic_field_types: &HashMap<String, crate::semantic::support::types::WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        if mcp_call.callee.root_keyword() != Some(mcp_call.operation.expected_root()) || mcp_call.callee.accesses.len() != 1 {
            let issue_key = (context.clone(), mcp_call.operation.expected_root());

            if self.invalid_keyword_reference_roots.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::InvalidKeywordReferenceRoot {
                        keyword: mcp_call.operation.expected_root(),
                        context: context.clone(),
                    },
                    Some(mcp_call.callee.span),
                );
            }
        } else if let Some(target_name) = mcp_call.target_name() {
            match mcp_call.operation {
                crate::dsl::McpCallOperation::Read => self.validate_resource_call_name(target_name, context.clone(), mcp_call.callee.span),
                crate::dsl::McpCallOperation::Render => self.validate_prompt_call_name(target_name, context.clone(), mcp_call.callee.span),
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

    fn validate_resource_call_name(&mut self, resource_name: &str, context: ValidationContext, span: SourceSpan) {
        if self.validation_index.resource_names.contains(resource_name) {
            return;
        }

        let issue_key = (context.clone(), resource_name.to_string());

        if self.unknown_resource_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::UnknownResourceReference {
                    resource_name: resource_name.to_string(),
                    context,
                },
                Some(span),
            );
        }
    }

    fn validate_prompt_call_name(&mut self, prompt_name: &str, context: ValidationContext, span: SourceSpan) {
        if self.validation_index.prompt_names.contains(prompt_name) {
            return;
        }

        let issue_key = (context.clone(), prompt_name.to_string());

        if self.unknown_prompt_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::UnknownPromptReference {
                    prompt_name: prompt_name.to_string(),
                    context,
                },
                Some(span),
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
            return;
        };

        if reference_root_keyword == ReferenceKeyword::Secrets && secret_reference_policy == SecretReferencePolicy::Forbid {
            self.push_secret_reference_leak(reference, context.clone());
        }

        let Some(_) = reference.first_access() else {
            let issue_key = (context.clone(), reference_root_keyword);

            if self.invalid_keyword_reference_roots.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::InvalidKeywordReferenceRoot {
                        keyword: reference_root_keyword,
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

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

    fn validate_agent_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_agent_name = reference
            .accesses
            .first()
            .expect("agent reference should include first field after root")
            .field
            .as_str();

        if !self.validate_agent_reference_name(referenced_agent_name, context.clone(), Some(reference.span)) {
            return;
        }

        let referenced_agent_output_type = self
            .validation_index
            .agent_output_types
            .get(referenced_agent_name)
            .and_then(Clone::clone);

        if reference.accesses.len() == 1 {
            if context == ValidationContext::Output && referenced_agent_output_type.is_none() {
                self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference.span);
            }

            return;
        }

        let Some(agent_output_type) = referenced_agent_output_type else {
            self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference.span);

            return;
        };

        self.validate_reference_path(reference, 1, agent_output_type, context);
    }

    fn push_missing_agent_output_type_reference_issue(
        &mut self,
        referenced_agent_name: &str,
        context: ValidationContext,
        reference_span: SourceSpan,
    ) {
        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.missing_agent_output_type_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::MissingAgentOutputTypeForFieldReference {
                    agent_name: referenced_agent_name.to_owned(),
                    context,
                },
                Some(reference_span),
            );
        }
    }

    fn validate_dynamic_reference(
        &mut self,
        reference: &Reference,
        dynamic_field_types: &HashMap<String, crate::semantic::support::types::WorkflowType>,
        context: ValidationContext,
    ) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("dynamic reference should include first field after root")
            .field
            .as_str();

        let Some(dynamic_field_type) = dynamic_field_types.get(referenced_field_name) else {
            if dynamic_field_types.is_empty() {
                if self.missing_dynamic_declaration_contexts.insert(context.clone()) {
                    self.validation_report
                        .push_issue_with_span(ValidationIssue::MissingDynamicDeclaration { context }, Some(reference.span));
                }

                return;
            }

            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_dynamic_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownDynamicFieldReference {
                        field_name: referenced_field_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.accesses.len() == 1 {
            return;
        }

        self.validate_workflow_type_reference_path(reference, 1, dynamic_field_type.clone(), context);
    }

    fn validate_input_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("input reference should include first field after root")
            .field
            .as_str();

        let Some(input_field_types) = self.validation_index.input_field_types.as_ref() else {
            if self.missing_input_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(ValidationIssue::MissingInputDeclaration { context }, Some(reference.span));
            }

            return;
        };

        let Some(input_field_type) = input_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_input_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownInputFieldReference {
                        field_name: referenced_field_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.accesses.len() == 1 {
            return;
        }

        self.validate_reference_path(reference, 1, input_field_type.clone(), context);
    }

    fn validate_secrets_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("secrets reference should include first field after root")
            .field
            .as_str();

        let Some(secrets_field_types) = self.validation_index.secrets_field_types.as_ref() else {
            if self.missing_secrets_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(ValidationIssue::MissingSecretsDeclaration { context }, Some(reference.span));
            }

            return;
        };

        let Some(secrets_field_type) = secrets_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_secrets_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownSecretsFieldReference {
                        field_name: referenced_field_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.accesses.len() == 1 {
            return;
        }

        self.validate_reference_path(reference, 1, secrets_field_type.clone(), context);
    }

    fn validate_reference_path(
        &mut self,
        reference: &Reference,
        path_start_index: usize,
        start_type: TypeExpression,
        context: ValidationContext,
    ) {
        let mut candidate_types = vec![start_type];

        for reference_access in reference.accesses.iter().skip(path_start_index) {
            if candidate_types.iter().any(TypeExpression::can_be_null) && !reference_access.optional {
                self.push_missing_optional_reference_access(reference, reference_access.field.as_str(), context.clone());

                return;
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                self.collect_next_types_for_field(candidate_type, reference_access.field.as_str(), &mut next_candidate_types);
            }

            if reference_access.optional {
                next_candidate_types.push(TypeExpression::Null);
            }

            if next_candidate_types.is_empty() {
                let reference_path = self.reference_to_string(reference);
                let issue_key = (context.clone(), reference_path.clone(), reference_access.field.clone());

                if self.invalid_reference_paths.insert(issue_key) {
                    self.validation_report.push_issue_with_span(
                        ValidationIssue::InvalidReferencePath {
                            reference_path,
                            invalid_field: reference_access.field.clone(),
                            context,
                        },
                        Some(reference.span),
                    );
                }

                return;
            }

            candidate_types = next_candidate_types;
        }
    }

    fn validate_workflow_type_reference_path(
        &mut self,
        reference: &Reference,
        path_start_index: usize,
        start_type: crate::semantic::support::types::WorkflowType,
        context: ValidationContext,
    ) {
        let mut candidate_types = vec![start_type];

        for reference_access in reference.accesses.iter().skip(path_start_index) {
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
                let reference_path = self.reference_to_string(reference);
                let issue_key = (context.clone(), reference_path.clone(), reference_access.field.clone());

                if self.invalid_reference_paths.insert(issue_key) {
                    self.validation_report.push_issue_with_span(
                        ValidationIssue::InvalidReferencePath {
                            reference_path,
                            invalid_field: reference_access.field.clone(),
                            context,
                        },
                        Some(reference.span),
                    );
                }

                return;
            }

            candidate_types = next_candidate_types;
        }
    }

    fn push_missing_optional_reference_access(&mut self, reference: &Reference, field_name: &str, context: ValidationContext) {
        let reference_path = self.reference_to_string(reference);
        let issue_key = (context.clone(), reference_path.clone(), field_name.to_owned());

        if self.missing_optional_reference_accesses.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::MissingOptionalReferenceAccess {
                    reference_path,
                    field_name: field_name.to_owned(),
                    context,
                },
                Some(reference.span),
            );
        }
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
                let generated_span = SourceSpan {
                    start: SourcePosition { line: 1, column: 1 },
                    end: SourcePosition { line: 1, column: 1 },
                };
                let Some(schema_type) = self.validation_index.schema_type_expression(schema_name, generated_span) else {
                    return;
                };

                self.collect_next_types_for_field(&schema_type, field_name, next_candidate_types);
            }
            TypeExpression::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    next_candidate_types.extend(
                        cases
                            .iter()
                            .map(|variant_case| TypeExpression::StringEnum(variant_case.name.clone())),
                    );
                }
            }
            TypeExpression::Union(type_expressions) => {
                for type_expression in type_expressions {
                    self.collect_next_types_for_field(type_expression, field_name, next_candidate_types);
                }
            }
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => {}
        }
    }

    fn collect_next_workflow_types_for_field(
        candidate_type: &crate::semantic::support::types::WorkflowType,
        field_name: &str,
        next_candidate_types: &mut Vec<crate::semantic::support::types::WorkflowType>,
    ) {
        match candidate_type {
            crate::semantic::support::types::WorkflowType::Object(fields) => {
                if let Some(field_type) = fields.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            crate::semantic::support::types::WorkflowType::Union(union_members) => {
                for union_member in union_members {
                    Self::collect_next_workflow_types_for_field(union_member, field_name, next_candidate_types);
                }
            }
            crate::semantic::support::types::WorkflowType::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    next_candidate_types.extend(
                        cases
                            .keys()
                            .cloned()
                            .map(|case_name| crate::semantic::support::types::WorkflowType::StringEnum(vec![case_name])),
                    );
                }
            }
            crate::semantic::support::types::WorkflowType::Any
            | crate::semantic::support::types::WorkflowType::String
            | crate::semantic::support::types::WorkflowType::Integer
            | crate::semantic::support::types::WorkflowType::Float
            | crate::semantic::support::types::WorkflowType::Boolean
            | crate::semantic::support::types::WorkflowType::Null
            | crate::semantic::support::types::WorkflowType::AnyObject
            | crate::semantic::support::types::WorkflowType::StringEnum(_)
            | crate::semantic::support::types::WorkflowType::Array {
                item_type: _,
                fixed_length: _,
            }
            | crate::semantic::support::types::WorkflowType::Tuple(_) => {}
        }
    }

    fn reference_to_string(&self, reference: &Reference) -> String {
        reference.render_path()
    }

    fn push_secret_reference_leak(&mut self, reference: &Reference, context: ValidationContext) {
        let reference_path = self.reference_to_string(reference);
        let issue_key = (context.clone(), reference_path.clone());

        if self.secret_reference_leaks.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::SecretReferenceInLlmContext { reference_path, context },
                Some(reference.span),
            );
        }
    }

    fn validate_agent_reference_name(&mut self, referenced_agent_name: &str, context: ValidationContext, span: Option<SourceSpan>) -> bool {
        if self.validation_index.agent_names.contains(referenced_agent_name) {
            return true;
        }

        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.unknown_agent_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::UnknownAgentReference {
                    referenced_agent: referenced_agent_name.to_owned(),
                    context,
                },
                span,
            );
        }

        false
    }
}

fn workflow_type_can_be_null(workflow_type: &crate::semantic::support::types::WorkflowType) -> bool {
    match workflow_type {
        crate::semantic::support::types::WorkflowType::Null => true,
        crate::semantic::support::types::WorkflowType::Union(union_members) => union_members.iter().any(workflow_type_can_be_null),
        crate::semantic::support::types::WorkflowType::Any
        | crate::semantic::support::types::WorkflowType::String
        | crate::semantic::support::types::WorkflowType::Integer
        | crate::semantic::support::types::WorkflowType::Float
        | crate::semantic::support::types::WorkflowType::Boolean
        | crate::semantic::support::types::WorkflowType::AnyObject
        | crate::semantic::support::types::WorkflowType::StringEnum(_)
        | crate::semantic::support::types::WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | crate::semantic::support::types::WorkflowType::Tuple(_)
        | crate::semantic::support::types::WorkflowType::Object(_)
        | crate::semantic::support::types::WorkflowType::Variant {
            discriminator: _,
            cases: _,
        } => false,
    }
}
