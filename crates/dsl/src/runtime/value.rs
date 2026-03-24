use crate::ast::{AccessOperator, Expression, FunctionExpression, ReferenceExpression, ReferenceRoot, StringFragment, StringTemplate};
use crate::compiler::CompiledWorkflow;
use crate::error::WorkflowError;
use crate::runtime::{StoredContext, WorkflowState};
use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;

pub(crate) fn evaluate_plain_expression(
    expression: &Expression,
    workflow: &CompiledWorkflow,
    state: &WorkflowState,
    local_values: &BTreeMap<String, Value>,
) -> Result<Value, WorkflowError> {
    match expression {
        Expression::Array(items) => items
            .iter()
            .map(|item| evaluate_plain_expression(item, workflow, state, local_values))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Expression::Boolean(boolean_value) => Ok(Value::Bool(*boolean_value)),
        Expression::Function(FunctionExpression::Context(reference)) => serialize_context_value(reference, state),
        Expression::Function(FunctionExpression::Compact(_)) => {
            Err(WorkflowError::execution("compact(...) requires asynchronous evaluation"))
        }
        Expression::Null => Ok(Value::Null),
        Expression::Number(number_literal) => parse_number(number_literal),
        Expression::Object(fields) => {
            let mut object_value = Map::new();

            for field in fields {
                object_value.insert(
                    field.name.clone(),
                    evaluate_plain_expression(&field.value, workflow, state, local_values)?,
                );
            }

            Ok(Value::Object(object_value))
        }
        Expression::Reference(reference) => resolve_reference_value(reference, workflow, state, local_values),
        Expression::String(template) => Ok(Value::String(render_inline_string(template, workflow, state, local_values)?)),
    }
}

pub(crate) fn render_inline_string(
    template: &StringTemplate,
    workflow: &CompiledWorkflow,
    state: &WorkflowState,
    local_values: &BTreeMap<String, Value>,
) -> Result<String, WorkflowError> {
    let mut rendered_string = String::new();

    for fragment in &template.fragments {
        match fragment {
            StringFragment::Expression(expression) => {
                let value = evaluate_plain_expression(expression, workflow, state, local_values)?;
                rendered_string.push_str(&stringify_value(&value)?);
            }
            StringFragment::Text(text) => rendered_string.push_str(text),
        }
    }

    Ok(rendered_string)
}

pub(crate) fn stringify_value(value: &Value) -> Result<String, WorkflowError> {
    match value {
        Value::String(string_value) => Ok(string_value.clone()),
        Value::Null => Ok("null".to_string()),
        Value::Bool(boolean_value) => Ok(boolean_value.to_string()),
        Value::Number(number_value) => Ok(number_value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value)
            .map_err(|error| WorkflowError::execution(format!("failed to stringify value for interpolation: {error}"))),
    }
}

pub(crate) fn serialize_context_value(reference: &ReferenceExpression, state: &WorkflowState) -> Result<Value, WorkflowError> {
    let ReferenceRoot::Agent(agent_name) = &reference.root else {
        return Err(WorkflowError::execution("context(...) requires an agent reference"));
    };
    let agent_result = state
        .agent_results
        .get(agent_name)
        .ok_or_else(|| WorkflowError::execution(format!("missing agent result for '{agent_name}'")))?;

    match &agent_result.context {
        StoredContext::Many(contexts) => contexts
            .iter()
            .map(serialize_context)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        StoredContext::Single(context) => serialize_context(context),
    }
}

pub(crate) fn serialize_context(context: &engine_ai_agent::Context) -> Result<Value, WorkflowError> {
    serde_json::to_value(context).map_err(|error| WorkflowError::execution(format!("failed to serialize agent context: {error}")))
}

pub(crate) fn resolve_reference_value(
    reference: &ReferenceExpression,
    _workflow: &CompiledWorkflow,
    state: &WorkflowState,
    local_values: &BTreeMap<String, Value>,
) -> Result<Value, WorkflowError> {
    let mut current_value = match &reference.root {
        ReferenceRoot::Agent(agent_name) => state
            .agent_results
            .get(agent_name)
            .map(|agent_result| agent_result.output.clone())
            .ok_or_else(|| WorkflowError::execution(format!("agent result '{agent_name}' is not available")))?,
        ReferenceRoot::Input(field_name) => lookup_object_property(&state.inputs, field_name, "input")?,
        ReferenceRoot::Local(variable_name) => local_values
            .get(variable_name)
            .cloned()
            .ok_or_else(|| WorkflowError::execution(format!("local value '{variable_name}' is not available")))?,
        ReferenceRoot::Secrets(secret_name) => state
            .secrets
            .get(secret_name)
            .cloned()
            .ok_or_else(|| WorkflowError::execution(format!("secret value '{secret_name}' is not available")))?,
    };

    for path_segment in &reference.path {
        current_value = apply_path_segment(current_value, path_segment.operator, &path_segment.property_name)?;
    }

    Ok(current_value)
}

fn lookup_object_property(container: &Value, property_name: &str, scope_name: &str) -> Result<Value, WorkflowError> {
    container
        .as_object()
        .and_then(|object_value| object_value.get(property_name))
        .cloned()
        .ok_or_else(|| WorkflowError::execution(format!("{scope_name} value '{property_name}' is not available")))
}

fn apply_path_segment(value: Value, operator: AccessOperator, property_name: &str) -> Result<Value, WorkflowError> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .map(|item| apply_path_segment(item, operator, property_name))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Null if operator == AccessOperator::Safe => Ok(Value::Null),
        Value::Object(object_value) => object_value
            .get(property_name)
            .cloned()
            .ok_or_else(|| WorkflowError::execution(format!("property '{property_name}' does not exist on runtime object"))),
        other_value => Err(WorkflowError::execution(format!(
            "cannot access property '{property_name}' on runtime value {other_value}"
        ))),
    }
}

fn parse_number(number_literal: &str) -> Result<Value, WorkflowError> {
    let normalized_number = number_literal.replace('_', "");

    if let Ok(integer_value) = normalized_number.parse::<i64>() {
        return Ok(Value::Number(Number::from(integer_value)));
    }

    let float_value = normalized_number
        .parse::<f64>()
        .map_err(|error| WorkflowError::execution(format!("failed to parse numeric literal '{number_literal}': {error}")))?;
    let number_value = Number::from_f64(float_value)
        .ok_or_else(|| WorkflowError::execution(format!("numeric literal '{number_literal}' is not finite")))?;

    Ok(Value::Number(number_value))
}
