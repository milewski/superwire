use crate::dsl::{CallArgument, Expression, FunctionCall, Reference, ReferenceKeyword, ReferenceRoot, StringTemplatePart};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::types::parse_number_literal;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub input_values: Map<String, Value>,
    pub secret_values: Map<String, Value>,
    pub agent_outputs: HashMap<String, Value>,
    pub agent_contexts: HashMap<String, Value>,
    pub local_bindings: HashMap<String, Value>,
}

pub fn evaluate_expression(
    expression: &Expression,
    evaluation_context: &EvaluationContext,
    context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    match expression {
        Expression::StringLiteral(string_literal) => Ok(Value::String(string_literal.clone())),
        Expression::StringTemplate(string_template) => {
            let mut rendered_template = String::new();

            for string_template_part in &string_template.parts {
                match string_template_part {
                    StringTemplatePart::Text(template_text) => {
                        rendered_template.push_str(template_text);
                    }
                    StringTemplatePart::Interpolation(interpolation_expression) => {
                        let interpolation_value = evaluate_expression(interpolation_expression, evaluation_context, context)?;

                        rendered_template.push_str(&render_template_value(&interpolation_value));
                    }
                }
            }

            Ok(Value::String(rendered_template))
        }
        Expression::NumberLiteral(number_literal) => Ok(Value::Number(parse_number_literal(number_literal)?)),
        Expression::BooleanLiteral(boolean_literal) => Ok(Value::Bool(*boolean_literal)),
        Expression::NullLiteral => Ok(Value::Null),
        Expression::Reference(reference) => evaluate_reference(reference, evaluation_context, context),
        Expression::FunctionCall(function_call) => evaluate_function_call(function_call, evaluation_context, context),
        Expression::ArrayLiteral(array_items) => {
            let mut evaluated_items = Vec::with_capacity(array_items.len());

            for array_item in array_items {
                evaluated_items.push(evaluate_expression(array_item, evaluation_context, context)?);
            }

            Ok(Value::Array(evaluated_items))
        }
        Expression::ObjectLiteral(object_fields) => {
            let mut evaluated_fields = Map::new();

            for object_field in object_fields {
                let field_value = evaluate_expression(&object_field.value, evaluation_context, context)?;
                evaluated_fields.insert(object_field.name.clone(), field_value);
            }

            Ok(Value::Object(evaluated_fields))
        }
    }
}

fn evaluate_reference(reference: &Reference, evaluation_context: &EvaluationContext, context: &str) -> Result<Value, WorkflowRuntimeError> {
    let (mut current_value, access_start_index) = resolve_reference_root(reference, evaluation_context, context)?;

    for reference_access in reference.accesses.iter().skip(access_start_index) {
        if current_value.is_null() && reference_access.optional {
            return Ok(Value::Null);
        }

        let Some(object_fields) = current_value.as_object() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!(
                    "reference path `{}.{}` cannot access field on non-object value",
                    render_reference(reference),
                    reference_access.field
                ),
            });
        };

        let Some(next_value) = object_fields.get(&reference_access.field) else {
            if reference_access.optional {
                return Ok(Value::Null);
            }

            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!(
                    "reference path `{}` is missing field `{}`",
                    render_reference(reference),
                    reference_access.field
                ),
            });
        };

        current_value = next_value.clone();
    }

    Ok(current_value)
}

fn evaluate_function_call(
    function_call: &FunctionCall,
    evaluation_context: &EvaluationContext,
    context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let function_name = function_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "function call must use identifier root".to_string(),
        })?;

    if function_name == "context" {
        return evaluate_context_call(function_call, evaluation_context, context);
    }

    Err(WorkflowRuntimeError::UnsupportedFeature {
        feature: format!("function `{function_name}` is not supported by runtime evaluator"),
    })
}

fn evaluate_context_call(
    function_call: &FunctionCall,
    evaluation_context: &EvaluationContext,
    context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let Some(agent_reference_expression) = context_call_agent_argument(function_call) else {
        return Err(WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "context(...) requires one agent reference argument".to_string(),
        });
    };

    let Expression::Reference(agent_reference) = agent_reference_expression else {
        return Err(WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "context(...) requires an `agent.<name>` reference".to_string(),
        });
    };

    if agent_reference.root.keyword() != Some(ReferenceKeyword::Agent) {
        return Err(WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "context(...) only supports `agent.<name>` references".to_string(),
        });
    }

    let Some(agent_name_access) = agent_reference.accesses.first() else {
        return Err(WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "context(...) requires `agent.<name>` with a concrete agent name".to_string(),
        });
    };

    let Some(agent_context_value) = evaluation_context.agent_contexts.get(&agent_name_access.field) else {
        return Err(WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!("context for agent `{}` is not available yet", agent_name_access.field),
        });
    };

    Ok(agent_context_value.clone())
}

fn context_call_agent_argument(function_call: &FunctionCall) -> Option<&Expression> {
    for call_argument in &function_call.arguments {
        match call_argument {
            CallArgument::Positional(expression) => {
                return Some(expression);
            }
            CallArgument::Named(named_argument) if named_argument.name == "agent" => {
                return Some(&named_argument.value);
            }
            CallArgument::Named(_) => {}
        }
    }

    None
}

fn resolve_reference_root(
    reference: &Reference,
    evaluation_context: &EvaluationContext,
    context: &str,
) -> Result<(Value, usize), WorkflowRuntimeError> {
    match &reference.root {
        ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
            if reference.accesses.is_empty() {
                return Ok((Value::Object(evaluation_context.input_values.clone()), 0));
            }

            let input_field_name = &reference.accesses[0].field;
            let Some(input_field_value) = evaluation_context.input_values.get(input_field_name) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown input field `{input_field_name}`"),
                });
            };

            Ok((input_field_value.clone(), 1))
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
            if reference.accesses.is_empty() {
                return Ok((Value::Object(evaluation_context.secret_values.clone()), 0));
            }

            let secret_field_name = &reference.accesses[0].field;
            let Some(secret_field_value) = evaluation_context.secret_values.get(secret_field_name) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown secret field `{secret_field_name}`"),
                });
            };

            Ok((secret_field_value.clone(), 1))
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Agent) => {
            if reference.accesses.is_empty() {
                let mut all_agent_outputs = Map::new();

                for (agent_name, agent_output) in &evaluation_context.agent_outputs {
                    all_agent_outputs.insert(agent_name.clone(), agent_output.clone());
                }

                return Ok((Value::Object(all_agent_outputs), 0));
            }

            let agent_name = &reference.accesses[0].field;
            let Some(agent_output_value) = evaluation_context.agent_outputs.get(agent_name) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("agent `{agent_name}` output is not available"),
                });
            };

            Ok((agent_output_value.clone(), 1))
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Tool) => Err(WorkflowRuntimeError::UnsupportedFeature {
            feature: "`tool.*` runtime references are not yet supported".to_string(),
        }),
        ReferenceRoot::Identifier(identifier) => {
            let Some(local_binding_value) = evaluation_context.local_bindings.get(identifier) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!("unknown local binding `{identifier}`"),
                });
            };

            Ok((local_binding_value.clone(), 0))
        }
    }
}

fn render_template_value(value: &Value) -> String {
    if let Some(string_value) = value.as_str() {
        return string_value.to_string();
    }

    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn render_reference(reference: &Reference) -> String {
    let mut rendered = match &reference.root {
        ReferenceRoot::Keyword(reference_keyword) => reference_keyword.as_str().to_string(),
        ReferenceRoot::Identifier(identifier) => identifier.clone(),
    };

    for access in &reference.accesses {
        if access.optional {
            rendered.push_str("?.");
            rendered.push_str(&access.field);

            continue;
        }

        rendered.push('.');
        rendered.push_str(&access.field);
    }

    rendered
}

pub fn collect_agent_dependencies(expression: &Expression, agent_dependencies: &mut HashSet<String>) {
    match expression {
        Expression::Reference(reference) => {
            collect_reference_dependency(reference, agent_dependencies);
        }
        Expression::FunctionCall(function_call) => {
            collect_reference_dependency(&function_call.callee, agent_dependencies);

            for call_argument in &function_call.arguments {
                match call_argument {
                    CallArgument::Positional(argument_expression) => {
                        collect_agent_dependencies(argument_expression, agent_dependencies);
                    }
                    CallArgument::Named(named_argument) => {
                        collect_agent_dependencies(&named_argument.value, agent_dependencies);
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_items) => {
            for array_item in array_items {
                collect_agent_dependencies(array_item, agent_dependencies);
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            for object_field in object_fields {
                collect_agent_dependencies(&object_field.value, agent_dependencies);
            }
        }
        Expression::StringTemplate(string_template) => {
            for template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = template_part {
                    collect_agent_dependencies(interpolation_expression, agent_dependencies);
                }
            }
        }
        Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
    }
}

fn collect_reference_dependency(reference: &Reference, agent_dependencies: &mut HashSet<String>) {
    if reference.root.keyword() != Some(ReferenceKeyword::Agent) {
        return;
    }

    let Some(first_access) = reference.accesses.first() else {
        return;
    };

    agent_dependencies.insert(first_access.field.clone());
}
