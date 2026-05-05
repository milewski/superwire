use crate::dsl::{Expression, Reference, ReferenceKeyword, ReferenceRoot, StringTemplatePart, ToolCall};
use crate::semantic::support::types::{ensure_type_matches, WorkflowType};
use crate::semantic::WorkflowSemanticError;
use std::collections::HashMap;

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

impl ToolCall {
    pub fn infer_type(&self, type_inference_context: &TypeInferenceContext, context: &str) -> Result<WorkflowType, WorkflowSemanticError> {
        let Some(tool_name) = self.callee.first_access_field() else {
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

    resolve_reference_access_path(&root_type, &reference.accesses, access_start_index, context)
}

fn resolve_reference_access_path(
    root_type: &WorkflowType,
    accesses: &[crate::dsl::ReferenceAccess],
    access_start_index: usize,
    context: &str,
) -> Result<WorkflowType, WorkflowSemanticError> {
    let mut candidate_types = vec![root_type.clone()];

    for access in accesses.iter().skip(access_start_index) {
        let mut next_candidate_types = Vec::new();

        for candidate_type in &candidate_types {
            collect_field_types(candidate_type, &access.field, &mut next_candidate_types);
        }

        if access.optional {
            next_candidate_types.push(WorkflowType::Null);
        }

        if next_candidate_types.is_empty() {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("invalid reference field access `{}`", access.field),
            });
        }

        candidate_types = next_candidate_types;
    }

    Ok(merge_types(candidate_types))
}

fn collect_field_types(candidate_type: &WorkflowType, field_name: &str, next_candidate_types: &mut Vec<WorkflowType>) {
    match candidate_type {
        WorkflowType::Object(fields) => {
            if let Some(field_type) = fields.get(field_name) {
                next_candidate_types.push(field_type.clone());
            }
        }
        WorkflowType::Union(union_members) => {
            for union_member in union_members {
                collect_field_types(union_member, field_name, next_candidate_types);
            }
        }
        WorkflowType::String
        | WorkflowType::Integer
        | WorkflowType::Float
        | WorkflowType::Boolean
        | WorkflowType::Null
        | WorkflowType::StringEnum(_)
        | WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | WorkflowType::Tuple(_) => {}
    }
}

fn merge_types(types: Vec<WorkflowType>) -> WorkflowType {
    if types.len() == 1 {
        return types[0].clone().normalize();
    }

    WorkflowType::Union(types).normalize()
}
