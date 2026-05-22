use crate::dsl::{Expression, MatchBranch, MatchExpression, Reference, ReferenceKeyword, ReferenceRoot, StringTemplatePart, ToolCall};
use crate::semantic::support::types::{ensure_type_matches, WorkflowType};
use crate::semantic::WorkflowSemanticError;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct TypeInferenceContext {
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub agent_output_types: HashMap<String, WorkflowType>,
    pub tool_input_types: HashMap<String, WorkflowType>,
    pub tool_binding_types: HashMap<String, WorkflowType>,
    pub tool_output_types: HashMap<String, WorkflowType>,
    pub local_binding_types: HashMap<String, WorkflowType>,
}

pub fn infer_expression_type(
    expression: &Expression,
    type_inference_context: &TypeInferenceContext,
    context: &str,
) -> Result<WorkflowType, WorkflowSemanticError> {
    expression.infer_type(type_inference_context, context)
}

impl Expression {
    pub fn infer_type(&self, type_inference_context: &TypeInferenceContext, context: &str) -> Result<WorkflowType, WorkflowSemanticError> {
        match self {
            Self::StringLiteral(_) => Ok(WorkflowType::String),
            Self::StringTemplate(string_template) => {
                for template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = template_part {
                        let _ = interpolation_expression.infer_type(type_inference_context, context)?;
                    }
                }

                Ok(WorkflowType::String)
            }
            Self::NumberLiteral(number_literal) => {
                let normalized_number_literal = number_literal.replace('_', "");

                if normalized_number_literal.contains('.') {
                    return Ok(WorkflowType::Float);
                }

                Ok(WorkflowType::Integer)
            }
            Self::BooleanLiteral(_) => Ok(WorkflowType::Boolean),
            Self::NullLiteral => Ok(WorkflowType::Null),
            Self::Reference(reference) => infer_reference_type(reference, type_inference_context, context),
            Self::FunctionCall(function_call) => {
                function_call.infer_builtin_type(type_inference_context, context, &|expression, type_inference_context, context| {
                    expression.infer_type(type_inference_context, context)
                })
            }
            Self::ToolCall(tool_call) => tool_call.infer_type(type_inference_context, context),
            Self::McpCall(mcp_call) => {
                for parameter_field in &mcp_call.parameter_fields {
                    let _ = parameter_field.value.infer_type(type_inference_context, context)?;
                }

                Ok(WorkflowType::String)
            }
            Self::NullFallback(null_fallback) => {
                let value_type = null_fallback.value.infer_type(type_inference_context, context)?;

                if !value_type.can_be_null() {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("left side of `??` must be nullable, found {value_type}"),
                    });
                }

                let inner_type = value_type.without_null();
                let fallback_type = null_fallback.fallback.infer_type(type_inference_context, context)?;

                if !ensure_type_matches(&inner_type, &fallback_type) {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("fallback expects {inner_type}, found {fallback_type}"),
                    });
                }

                Ok(inner_type)
            }
            Self::VariantProjection(variant_projection) => {
                let value_type = infer_reference_type(&variant_projection.value, type_inference_context, context)?;
                let inner_type = value_type.without_null();
                let Some(projected_type) =
                    inner_type.variant_case_field_type(&variant_projection.case_name, &variant_projection.field_path)
                else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("invalid variant projection case `{}`", variant_projection.case_name),
                    });
                };

                Ok(WorkflowType::nullable(projected_type))
            }
            Self::Match(match_expression) => match_expression.infer_type(type_inference_context, context),
            Self::ArrayLiteral(array_items) => {
                if array_items.is_empty() {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: "empty array literals are not supported in statically-typed workflow expressions".to_string(),
                    });
                }

                let mut item_types = Vec::with_capacity(array_items.len());

                for array_item in array_items {
                    item_types.push(array_item.infer_type(type_inference_context, context)?);
                }

                let merged_item_type = merge_types(item_types);

                Ok(WorkflowType::Array {
                    item_type: Box::new(merged_item_type),
                    fixed_length: None,
                })
            }
            Self::ObjectLiteral(object_fields) => {
                let mut field_types = std::collections::BTreeMap::new();

                for object_field in object_fields {
                    let field_type = object_field.value.infer_type(type_inference_context, context)?;
                    field_types.insert(object_field.name.clone(), field_type);
                }

                Ok(WorkflowType::Object(field_types))
            }
        }
    }
}

impl MatchExpression {
    fn infer_type(&self, type_inference_context: &TypeInferenceContext, context: &str) -> Result<WorkflowType, WorkflowSemanticError> {
        let matched_type = self.value.infer_type(type_inference_context, context)?;
        let matched_inner_type = matched_type.without_null();
        let mut branch_types = Vec::new();
        let has_fallback_branch = self.branches.iter().any(MatchBranch::is_fallback);

        self.validate_nullable_coverage(&matched_type, has_fallback_branch, context)?;
        self.validate_variant_coverage(&matched_inner_type, has_fallback_branch, context)?;

        for branch in &self.branches {
            branch_types.push(branch.infer_type(&matched_inner_type, type_inference_context, context)?);
        }

        if branch_types.is_empty() {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: "match expression requires at least one branch".to_string(),
            });
        }

        let first_branch_type = branch_types[0].clone();

        for branch_type in branch_types.iter().skip(1) {
            if ensure_type_matches(&first_branch_type, branch_type) {
                continue;
            }

            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("match branches return incompatible types: {first_branch_type} and {branch_type}"),
            });
        }

        Ok(first_branch_type)
    }

    fn validate_nullable_coverage(
        &self,
        matched_type: &WorkflowType,
        has_fallback_branch: bool,
        context: &str,
    ) -> Result<(), WorkflowSemanticError> {
        if !matched_type.can_be_null() || has_fallback_branch {
            return Ok(());
        }

        Err(WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: "nullable match expression requires a `_` fallback branch".to_string(),
        })
    }

    fn validate_variant_coverage(
        &self,
        matched_type: &WorkflowType,
        has_fallback_branch: bool,
        context: &str,
    ) -> Result<(), WorkflowSemanticError> {
        if has_fallback_branch {
            return Ok(());
        }

        let Some(case_names) = matched_type.variant_case_names() else {
            return Ok(());
        };

        let matched_case_names = self.branches.iter().filter_map(MatchBranch::case_name).collect::<BTreeSet<_>>();
        let missing_case_names = case_names
            .iter()
            .filter(|case_name| !matched_case_names.contains(case_name.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        if missing_case_names.is_empty() {
            return Ok(());
        }

        Err(WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!("non-exhaustive match expression; missing cases: {}", missing_case_names.join(", ")),
        })
    }
}

impl MatchBranch {
    fn is_fallback(&self) -> bool {
        matches!(self, Self::Fallback { value: _, span: _ })
    }

    fn case_name(&self) -> Option<&str> {
        match self {
            Self::Variant {
                case_name,
                field_path: _,
                span: _,
            } => Some(case_name),
            Self::Fallback { value: _, span: _ } => None,
        }
    }

    fn infer_type(
        &self,
        matched_type: &WorkflowType,
        type_inference_context: &TypeInferenceContext,
        context: &str,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        match self {
            Self::Variant {
                case_name,
                field_path,
                span: _,
            } => matched_type
                .variant_case_field_type(case_name, field_path)
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("invalid match case `{case_name}`"),
                }),
            Self::Fallback { value, span: _ } => value.infer_type(type_inference_context, context),
        }
    }
}

impl ToolCall {
    pub fn infer_type(&self, type_inference_context: &TypeInferenceContext, context: &str) -> Result<WorkflowType, WorkflowSemanticError> {
        let Some(tool_name) = self.callee.tool_name() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: "tool call requires a tool name".to_string(),
            });
        };

        let Some(tool_output_type) = type_inference_context.tool_output_types.get(tool_name) else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("unknown tool call `tool.{tool_name}`"),
            });
        };

        self.validate_object_fields(
            tool_name,
            "input",
            &self.input_fields,
            type_inference_context.tool_input_types.get(tool_name),
            type_inference_context,
            context,
        )?;

        self.validate_object_fields(
            tool_name,
            "bindings",
            &self.binding_fields,
            type_inference_context.tool_binding_types.get(tool_name),
            type_inference_context,
            context,
        )?;

        Ok(tool_output_type.clone())
    }

    fn validate_object_fields(
        &self,
        tool_name: &str,
        field_group_name: &str,
        fields: &[crate::dsl::ObjectField],
        expected_type: Option<&WorkflowType>,
        type_inference_context: &TypeInferenceContext,
        context: &str,
    ) -> Result<(), WorkflowSemanticError> {
        let Some(WorkflowType::Object(expected_fields)) = expected_type else {
            if fields.is_empty() {
                return Ok(());
            }

            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("tool `tool.{tool_name}` does not declare `{field_group_name}` fields"),
            });
        };

        for expected_field_name in expected_fields.keys() {
            let Some(expected_field_type) = expected_fields.get(expected_field_name) else {
                continue;
            };

            if expected_field_type.can_be_null() {
                continue;
            }

            if fields.iter().any(|field| &field.name == expected_field_name) {
                continue;
            }

            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("tool `tool.{tool_name}` missing required `{field_group_name}` field `{expected_field_name}`"),
            });
        }

        for field in fields {
            let Some(expected_field_type) = expected_fields.get(&field.name) else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!(
                        "tool `tool.{tool_name}` does not declare `{field_group_name}` field `{}`",
                        field.name
                    ),
                });
            };

            if field.value.is_literal_compatible_with_workflow_type(expected_field_type) {
                continue;
            }

            let found_field_type = field.value.infer_type(type_inference_context, context)?;

            if ensure_type_matches(expected_field_type, &found_field_type) {
                continue;
            }

            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!(
                    "tool `tool.{tool_name}` `{field_group_name}` field `{}` expects {}, found {}",
                    field.name, expected_field_type, found_field_type
                ),
            });
        }

        Ok(())
    }
}

impl Expression {
    fn is_literal_compatible_with_workflow_type(&self, expected_type: &WorkflowType) -> bool {
        match (self, expected_type) {
            (Self::StringLiteral(string_literal), WorkflowType::StringEnum(enum_values)) => enum_values.contains(string_literal),
            (Self::StringLiteral(_), WorkflowType::String) => true,
            (Self::NumberLiteral(number_literal), WorkflowType::Float) => number_literal.replace('_', "").contains('.'),
            (Self::NumberLiteral(number_literal), WorkflowType::Integer) => !number_literal.replace('_', "").contains('.'),
            (Self::BooleanLiteral(_), WorkflowType::Boolean) | (Self::NullLiteral, WorkflowType::Null) => true,
            (Self::ArrayLiteral(array_items), WorkflowType::Array { item_type, fixed_length }) => {
                fixed_length.is_none_or(|expected_length| {
                    u64::try_from(array_items.len()).is_ok_and(|actual_length| actual_length == expected_length)
                }) && array_items
                    .iter()
                    .all(|array_item| array_item.is_literal_compatible_with_workflow_type(item_type))
            }
            (expression, WorkflowType::Union(union_members)) => union_members
                .iter()
                .any(|union_member| expression.is_literal_compatible_with_workflow_type(union_member)),
            _ => false,
        }
    }
}

fn infer_reference_type(
    reference: &Reference,
    type_inference_context: &TypeInferenceContext,
    context: &str,
) -> Result<WorkflowType, WorkflowSemanticError> {
    let (root_type, access_start_index) = match &reference.root {
        ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
            let Some(input_type) = &type_inference_context.input_type else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "input reference used without input declaration".to_string(),
                });
            };

            (input_type.clone(), 0)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Dynamic) => {
            let Some(dynamic_field_name) = reference.first_access_field() else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "dynamic reference requires a field name".to_string(),
                });
            };

            let Some(dynamic_field_type) = type_inference_context.local_binding_types.get(dynamic_field_name) else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown dynamic field `{dynamic_field_name}`"),
                });
            };

            (dynamic_field_type.clone(), 1)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
            let Some(secrets_type) = &type_inference_context.secrets_type else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "secrets reference used without secrets declaration".to_string(),
                });
            };

            (secrets_type.clone(), 0)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Agent) => {
            let Some(agent_name) = reference.first_access_field() else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "agent reference requires an agent name".to_string(),
                });
            };

            let Some(agent_output_type) = type_inference_context.agent_output_types.get(agent_name) else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown agent reference `{agent_name}`"),
                });
            };

            (agent_output_type.clone(), 1)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Tool) => {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`tool.*` references are not supported in typed output expressions".to_string(),
            });
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Resource) => {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`resource.*` references are not supported outside `read resource.*`".to_string(),
            });
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Prompt) => {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`prompt.*` references are not supported outside `render prompt.*`".to_string(),
            });
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Model) => {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`model.*` references are only supported in agent model properties".to_string(),
            });
        }
        ReferenceRoot::Identifier(identifier) => {
            let Some(local_binding_type) = type_inference_context.local_binding_types.get(identifier) else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown identifier `{identifier}`"),
                });
            };

            (local_binding_type.clone(), 0)
        }
    };

    reference.resolve_workflow_type_access_path(&root_type, access_start_index, context)
}

impl Reference {
    fn resolve_workflow_type_access_path(
        &self,
        root_type: &WorkflowType,
        access_start_index: usize,
        context: &str,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        let mut candidate_types = vec![root_type.clone()];

        for reference_access in self.accesses_from(access_start_index) {
            let mut next_candidate_types = Vec::new();

            if candidate_types.iter().any(WorkflowType::can_be_null) && !reference_access.optional {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("cannot access `{}` through a nullable value; use `?.`", reference_access.field),
                });
            }

            for candidate_type in &candidate_types {
                if let Some(field_type) = candidate_type.without_null().field_type(&reference_access.field) {
                    next_candidate_types.push(field_type);
                }
            }

            if reference_access.optional {
                next_candidate_types.push(WorkflowType::Null);
            }

            if next_candidate_types.is_empty() {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("invalid reference field access `{}`", reference_access.field),
                });
            }

            candidate_types = next_candidate_types;
        }

        Ok(merge_types(candidate_types))
    }
}

fn merge_types(types: Vec<WorkflowType>) -> WorkflowType {
    if types.len() == 1 {
        return types[0].clone().normalize();
    }

    WorkflowType::Union(types).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{SourcePosition, SourceSpan};
    use std::collections::BTreeMap;

    fn empty_type_inference_context() -> TypeInferenceContext {
        TypeInferenceContext {
            input_type: None,
            secrets_type: None,
            agent_output_types: HashMap::new(),
            tool_input_types: HashMap::new(),
            tool_binding_types: HashMap::new(),
            tool_output_types: HashMap::new(),
            local_binding_types: HashMap::new(),
        }
    }

    fn source_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }

    fn event_variant_type() -> WorkflowType {
        WorkflowType::Variant {
            discriminator: "type".to_string(),
            cases: BTreeMap::from([
                ("created".to_string(), BTreeMap::from([("id".to_string(), WorkflowType::String)])),
                ("deleted".to_string(), BTreeMap::from([("id".to_string(), WorkflowType::String)])),
            ]),
        }
    }

    fn match_expression(branches: Vec<MatchBranch>) -> MatchExpression {
        MatchExpression {
            value: Box::new(Expression::Reference(Reference {
                root: ReferenceRoot::Identifier("event".to_string()),
                accesses: Vec::new(),
                span: source_span(),
            })),
            branches,
            span: source_span(),
        }
    }

    fn variant_branch(case_name: &str) -> MatchBranch {
        MatchBranch::Variant {
            case_name: case_name.to_string(),
            field_path: vec!["id".to_string()],
            span: source_span(),
        }
    }

    fn fallback_branch() -> MatchBranch {
        MatchBranch::Fallback {
            value: Expression::StringLiteral("fallback".to_string()),
            span: source_span(),
        }
    }

    #[test]
    fn rejects_non_exhaustive_variant_match_without_fallback() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context
            .local_binding_types
            .insert("event".to_string(), event_variant_type());
        let match_expression = match_expression(vec![variant_branch("created")]);
        let error = match_expression
            .infer_type(&type_inference_context, "test")
            .expect_err("non-exhaustive match should fail");

        assert!(error.to_string().contains("missing cases: deleted"));
    }

    #[test]
    fn accepts_exhaustive_variant_match_without_fallback() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context
            .local_binding_types
            .insert("event".to_string(), event_variant_type());
        let match_expression = match_expression(vec![variant_branch("created"), variant_branch("deleted")]);
        let inferred_type = match_expression
            .infer_type(&type_inference_context, "test")
            .expect("exhaustive match should infer");

        assert_eq!(inferred_type, WorkflowType::String);
    }

    #[test]
    fn rejects_nullable_variant_match_without_fallback() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context
            .local_binding_types
            .insert("event".to_string(), WorkflowType::nullable(event_variant_type()));
        let match_expression = match_expression(vec![variant_branch("created"), variant_branch("deleted")]);
        let error = match_expression
            .infer_type(&type_inference_context, "test")
            .expect_err("nullable match without fallback should fail");

        assert!(error.to_string().contains("requires a `_` fallback branch"));
    }

    #[test]
    fn accepts_nullable_variant_match_with_fallback() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context
            .local_binding_types
            .insert("event".to_string(), WorkflowType::nullable(event_variant_type()));
        let match_expression = match_expression(vec![variant_branch("created"), variant_branch("deleted"), fallback_branch()]);
        let inferred_type = match_expression
            .infer_type(&type_inference_context, "test")
            .expect("nullable match with fallback should infer");

        assert_eq!(inferred_type, WorkflowType::String);
    }

    #[test]
    fn tool_call_allows_missing_nullable_input_field() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context.tool_input_types.insert(
            "fetch_answers".to_string(),
            WorkflowType::Object(BTreeMap::from([
                ("project_id".to_string(), WorkflowType::Integer),
                ("task_group_id".to_string(), WorkflowType::nullable(WorkflowType::Integer)),
                (
                    "task_types".to_string(),
                    WorkflowType::Array {
                        item_type: Box::new(WorkflowType::StringEnum(vec!["open_written".to_string()])),
                        fixed_length: None,
                    },
                ),
            ])),
        );
        type_inference_context
            .tool_output_types
            .insert("fetch_answers".to_string(), WorkflowType::Object(BTreeMap::new()));

        let tool_call = ToolCall {
            callee: tool_reference("fetch_answers"),
            input_fields: vec![
                object_field("project_id", Expression::NumberLiteral("31".to_string())),
                object_field(
                    "task_types",
                    Expression::ArrayLiteral(vec![Expression::StringLiteral("open_written".to_string())]),
                ),
            ],
            binding_fields: Vec::new(),
            max_calls: None,
            span: source_span(),
        };

        let inferred_type = tool_call
            .infer_type(&type_inference_context, "test")
            .expect("missing nullable input field should infer");

        assert_eq!(inferred_type, WorkflowType::Object(BTreeMap::new()));
    }

    #[test]
    fn tool_call_allows_null_for_nullable_input_field() {
        let mut type_inference_context = empty_type_inference_context();
        type_inference_context.tool_input_types.insert(
            "fetch_answers".to_string(),
            WorkflowType::Object(BTreeMap::from([
                ("project_id".to_string(), WorkflowType::Integer),
                ("task_group_id".to_string(), WorkflowType::nullable(WorkflowType::Integer)),
            ])),
        );
        type_inference_context
            .tool_output_types
            .insert("fetch_answers".to_string(), WorkflowType::Object(BTreeMap::new()));

        let tool_call = ToolCall {
            callee: tool_reference("fetch_answers"),
            input_fields: vec![
                object_field("project_id", Expression::NumberLiteral("31".to_string())),
                object_field("task_group_id", Expression::NullLiteral),
            ],
            binding_fields: Vec::new(),
            max_calls: None,
            span: source_span(),
        };

        let inferred_type = tool_call
            .infer_type(&type_inference_context, "test")
            .expect("null nullable input field should infer");

        assert_eq!(inferred_type, WorkflowType::Object(BTreeMap::new()));
    }

    fn tool_reference(tool_name: &str) -> Reference {
        Reference {
            root: ReferenceRoot::Keyword(ReferenceKeyword::Tool),
            accesses: vec![crate::dsl::ReferenceAccess {
                field: tool_name.to_string(),
                optional: false,
            }],
            span: source_span(),
        }
    }

    fn object_field(field_name: &str, value: Expression) -> crate::dsl::ObjectField {
        crate::dsl::ObjectField {
            name: field_name.to_string(),
            value,
            span: source_span(),
        }
    }
}
