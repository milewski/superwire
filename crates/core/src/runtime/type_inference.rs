use crate::dsl::{Expression, Reference, ReferenceKeyword, ReferenceRoot, StringTemplatePart};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::functions::infer_builtin_function_type;
use crate::runtime::types::WorkflowType;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TypeInferenceContext {
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub agent_output_types: HashMap<String, WorkflowType>,
    pub local_binding_types: HashMap<String, WorkflowType>,
}

pub fn infer_expression_type(
    expression: &Expression,
    type_inference_context: &TypeInferenceContext,
    context: &str,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    match expression {
        Expression::StringLiteral(_) => Ok(WorkflowType::String),
        Expression::StringTemplate(string_template) => {
            for template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = template_part {
                    let _ = infer_expression_type(interpolation_expression, type_inference_context, context)?;
                }
            }

            Ok(WorkflowType::String)
        }
        Expression::NumberLiteral(number_literal) => {
            let normalized_number_literal = number_literal.replace('_', "");

            if normalized_number_literal.contains('.') {
                return Ok(WorkflowType::Float);
            }

            Ok(WorkflowType::Integer)
        }
        Expression::BooleanLiteral(_) => Ok(WorkflowType::Boolean),
        Expression::NullLiteral => Ok(WorkflowType::Null),
        Expression::Reference(reference) => infer_reference_type(reference, type_inference_context, context),
        Expression::FunctionCall(function_call) => {
            infer_builtin_function_type(function_call, type_inference_context, context, &infer_expression_type)
        }
        Expression::ArrayLiteral(array_items) => {
            if array_items.is_empty() {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "empty array literals are not supported in statically-typed workflow expressions".to_string(),
                });
            }

            let mut item_types = Vec::with_capacity(array_items.len());

            for array_item in array_items {
                item_types.push(infer_expression_type(array_item, type_inference_context, context)?);
            }

            let merged_item_type = merge_types(item_types);

            Ok(WorkflowType::Array {
                item_type: Box::new(merged_item_type),
                fixed_length: None,
            })
        }
        Expression::ObjectLiteral(object_fields) => {
            let mut field_types = std::collections::BTreeMap::new();

            for object_field in object_fields {
                let field_type = infer_expression_type(&object_field.value, type_inference_context, context)?;
                field_types.insert(object_field.name.clone(), field_type);
            }

            Ok(WorkflowType::Object(field_types))
        }
    }
}

fn infer_reference_type(
    reference: &Reference,
    type_inference_context: &TypeInferenceContext,
    context: &str,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    let (root_type, access_start_index) = match &reference.root {
        ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
            let Some(input_type) = &type_inference_context.input_type else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "input reference used without input declaration".to_string(),
                });
            };

            (input_type.clone(), 0)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
            let Some(secrets_type) = &type_inference_context.secrets_type else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "secrets reference used without secrets declaration".to_string(),
                });
            };

            (secrets_type.clone(), 0)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Agent) => {
            let Some(agent_name_access) = reference.accesses.first() else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "agent reference requires an agent name".to_string(),
                });
            };

            let Some(agent_output_type) = type_inference_context.agent_output_types.get(&agent_name_access.field) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown agent reference `{}`", agent_name_access.field),
                });
            };

            (agent_output_type.clone(), 1)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Tool) => {
            return Err(WorkflowRuntimeError::UnsupportedFeature {
                feature: "`tool.*` references are not supported in typed output expressions".to_string(),
            });
        }
        ReferenceRoot::Identifier(identifier) => {
            let Some(local_binding_type) = type_inference_context.local_binding_types.get(identifier) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown local binding `{identifier}`"),
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
) -> Result<WorkflowType, WorkflowRuntimeError> {
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
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
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
