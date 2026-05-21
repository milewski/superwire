use super::super::ast::{AgentDeclaration, Expression, ObjectField, SourceSpan};
use super::report::ValidationReport;
use crate::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::semantic::support::types::{ensure_type_matches, WorkflowType};
use crate::semantic::WorkflowSemanticIndex as ValidationIndex;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) fn validate_agent_tool_bindings(
    agent_declaration: &AgentDeclaration,
    tools_expression: &Expression,
    local_binding_types: &HashMap<String, WorkflowType>,
    validation_index: &ValidationIndex,
    base_type_inference_context: &TypeInferenceContext,
    validation_report: &mut ValidationReport,
) {
    let Expression::ArrayLiteral(tool_expressions) = tools_expression else {
        return;
    };

    let mut validator = AgentToolBindingValidator {
        agent_declaration,
        local_binding_types,
        validation_index,
        base_type_inference_context,
        validation_report,
    };

    for tool_expression in tool_expressions {
        validator.validate_tool_expression(tool_expression);
    }
}

struct AgentToolBindingValidator<'validation> {
    agent_declaration: &'validation AgentDeclaration,
    local_binding_types: &'validation HashMap<String, WorkflowType>,
    validation_index: &'validation ValidationIndex,
    base_type_inference_context: &'validation TypeInferenceContext,
    validation_report: &'validation mut ValidationReport,
}

impl AgentToolBindingValidator<'_> {
    fn validate_tool_expression(&mut self, tool_expression: &Expression) {
        let Some(tool_name) = tool_expression.direct_tool_name() else {
            return;
        };

        let Some(WorkflowType::Object(expected_binding_fields)) = self.validation_index.tool_binding_type(tool_name) else {
            return;
        };

        self.validate_binding_fields(tool_name, tool_expression.agent_tool_binding_fields(), expected_binding_fields);
    }

    fn validate_binding_fields(
        &mut self,
        tool_name: &str,
        binding_fields: &[ObjectField],
        expected_binding_fields: &BTreeMap<String, WorkflowType>,
    ) {
        self.validate_fixed_binding_overrides(tool_name, binding_fields);
        self.validate_self_references(tool_name, binding_fields);
        self.validate_required_bindings(tool_name, binding_fields, expected_binding_fields);
        self.validate_binding_field_types(tool_name, binding_fields, expected_binding_fields);
    }

    fn validate_fixed_binding_overrides(&mut self, tool_name: &str, binding_fields: &[ObjectField]) {
        let Some(fixed_names) = self.validation_index.tool_fixed_binding_names(tool_name) else {
            return;
        };

        for binding_field in binding_fields {
            if fixed_names.contains(&binding_field.name) {
                self.push_invalid_tool_binding(
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

    fn validate_required_bindings(
        &mut self,
        tool_name: &str,
        binding_fields: &[ObjectField],
        expected_binding_fields: &BTreeMap<String, WorkflowType>,
    ) {
        for expected_binding_name in expected_binding_fields.keys() {
            if binding_fields
                .iter()
                .any(|binding_field| &binding_field.name == expected_binding_name)
            {
                continue;
            }

            self.push_invalid_tool_binding(
                tool_name,
                format!("missing required bound argument `{expected_binding_name}`"),
                Some(self.agent_declaration.span),
            );
        }
    }

    fn validate_binding_field_types(
        &mut self,
        tool_name: &str,
        binding_fields: &[ObjectField],
        expected_binding_fields: &BTreeMap<String, WorkflowType>,
    ) {
        let mut type_inference_context = self.base_type_inference_context.clone();
        type_inference_context.local_binding_types.clone_from(self.local_binding_types);

        for binding_field in binding_fields {
            let Some(expected_binding_type) = expected_binding_fields.get(&binding_field.name) else {
                self.push_invalid_tool_binding(
                    tool_name,
                    format!("unknown bound argument `{}`", binding_field.name),
                    Some(self.agent_declaration.span),
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
                tool_name,
                format!(
                    "bound argument `{}` expects {}, found {}",
                    binding_field.name, expected_binding_type, actual_binding_type
                ),
                Some(self.agent_declaration.span),
            );
        }
    }

    fn validate_self_references(&mut self, tool_name: &str, binding_fields: &[ObjectField]) {
        for binding_field in binding_fields {
            self.validate_expression_self_reference(
                tool_name,
                &binding_field.name,
                "agent tool binding override",
                &binding_field.value,
                binding_field.span,
            );
        }

        let Some(fixed_binding_fields) = self
            .validation_index
            .tool_fixed_binding_fields(tool_name)
            .map(<[ObjectField]>::to_vec)
        else {
            return;
        };

        for fixed_binding_field in &fixed_binding_fields {
            self.validate_expression_self_reference(
                tool_name,
                &fixed_binding_field.name,
                "tool declaration binding",
                &fixed_binding_field.value,
                fixed_binding_field.span,
            );
        }
    }

    fn validate_expression_self_reference(
        &mut self,
        tool_name: &str,
        binding_name: &str,
        binding_source: &str,
        expression: &Expression,
        span: SourceSpan,
    ) {
        let mut referenced_agents = HashSet::new();

        expression.collect_agent_dependencies(&mut referenced_agents);

        if !referenced_agents.contains(&self.agent_declaration.name) {
            return;
        }

        self.push_invalid_tool_binding(
            tool_name,
            format!(
                "{binding_source} `{binding_name}` references `agent.{}` while `tool.{tool_name}` is attached to agent `{}`; \
                 an agent cannot call a tool that requires its own output because that output is only available after the agent finishes. \
                 Move `tool.{tool_name}` to a later agent that depends on `{}`, or bind `{binding_name}` from input, dynamic data, or a previous agent",
                self.agent_declaration.name, self.agent_declaration.name, self.agent_declaration.name
            ),
            Some(span),
        );
    }

    fn push_invalid_tool_binding(&mut self, tool_name: &str, message: String, span: Option<SourceSpan>) {
        self.validation_report
            .push_issue_with_span(self.agent_declaration.invalid_tool_binding_issue(tool_name, message), span);
    }
}

impl Expression {
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
