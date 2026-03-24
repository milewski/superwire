use crate::dsl::{
    AgentDeclaration, AgentProperty, CallArgument, Expression, FunctionCall, ProviderDeclaration, Reference, ReferenceKeyword,
    ReferenceRoot, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::types::{ExecutionScope, ModelBinding};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub(crate) fn evaluate_provider_settings(
    provider_declaration: &ProviderDeclaration,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Map<String, Value>, WorkflowRuntimeError> {
    let mut provider_settings = Map::new();

    for provider_property in &provider_declaration.properties {
        let property_context = format!("{evaluation_context} property '{}'", provider_property.name);
        let property_value = evaluate_expression(&provider_property.value, execution_scope, property_context.as_str())?;
        provider_settings.insert(provider_property.name.clone(), property_value);
    }

    Ok(provider_settings)
}

pub(crate) fn find_prompt_expression(agent_properties: &[AgentProperty]) -> Option<&Expression> {
    agent_properties.iter().find_map(|agent_property| {
        if let AgentProperty::Prompt(prompt_expression) = agent_property {
            Some(prompt_expression)
        } else {
            None
        }
    })
}

pub(crate) fn find_output_type_expression(agent_properties: &[AgentProperty]) -> Option<&TypeExpression> {
    agent_properties.iter().find_map(|agent_property| {
        if let AgentProperty::Output(output_type_expression) = agent_property {
            Some(output_type_expression)
        } else {
            None
        }
    })
}

pub(crate) fn extract_model_binding(agent_declaration: &AgentDeclaration) -> Result<ModelBinding, WorkflowRuntimeError> {
    let model_expression = agent_declaration
        .properties
        .iter()
        .find_map(|agent_property| {
            if let AgentProperty::Model(model_expression) = agent_property {
                Some(model_expression)
            } else {
                None
            }
        })
        .ok_or_else(|| WorkflowRuntimeError::MissingModelExpression {
            agent_name: agent_declaration.name.clone(),
        })?;

    let Expression::FunctionCall(model_call) = model_expression else {
        return Err(WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        });
    };

    if !model_call.callee.accesses.is_empty() {
        return Err(WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        });
    }

    let provider_name = model_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        })?
        .to_owned();

    let model_name = extract_model_name(model_call).ok_or_else(|| WorkflowRuntimeError::InvalidModelExpression {
        agent_name: agent_declaration.name.clone(),
    })?;

    Ok(ModelBinding { provider_name, model_name })
}

fn extract_model_name(model_call: &FunctionCall) -> Option<String> {
    for call_argument in &model_call.arguments {
        match call_argument {
            CallArgument::Positional(Expression::StringLiteral(model_name)) => {
                return Some(model_name.clone());
            }
            CallArgument::Named(named_argument) if named_argument.name == "model" => {
                let Expression::StringLiteral(model_name) = &named_argument.value else {
                    return None;
                };

                return Some(model_name.clone());
            }
            CallArgument::Named(_) | CallArgument::Positional(_) => {}
        }
    }

    None
}

pub(crate) fn evaluate_workflow_output(
    workflow: &Workflow,
    input_values: &Value,
    secret_values: &Value,
    agent_outputs_by_name: &HashMap<String, Value>,
) -> Result<Value, WorkflowRuntimeError> {
    let Some(output_declaration) = workflow.find_output() else {
        return Ok(Value::Object(Map::new()));
    };

    let execution_scope = ExecutionScope {
        input_values,
        secret_values,
        agent_outputs_by_name,
    };
    let mut output_object = Map::new();

    for output_field in &output_declaration.fields {
        let output_field_context = format!("workflow output field '{}'", output_field.name);
        let output_field_value = evaluate_expression(&output_field.value, &execution_scope, output_field_context.as_str())?;
        output_object.insert(output_field.name.clone(), output_field_value);
    }

    Ok(Value::Object(output_object))
}

pub(crate) fn evaluate_expression(
    expression: &Expression,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    match expression {
        Expression::StringLiteral(string_value) => Ok(Value::String(string_value.clone())),
        Expression::StringTemplate(string_template) => {
            let mut rendered_string = String::new();

            for string_template_part in &string_template.parts {
                match string_template_part {
                    StringTemplatePart::Text(text_value) => {
                        rendered_string.push_str(text_value);
                    }
                    StringTemplatePart::Interpolation(interpolation_expression) => {
                        let interpolation_value = evaluate_expression(interpolation_expression, execution_scope, evaluation_context)?;

                        rendered_string.push_str(render_value_as_text(&interpolation_value).as_str());
                    }
                }
            }

            Ok(Value::String(rendered_string))
        }
        Expression::NumberLiteral(number_literal) => {
            let normalized_literal = number_literal.replace('_', "");
            let parsed_number = normalized_literal
                .parse::<f64>()
                .map_err(|_| WorkflowRuntimeError::InvalidNumberLiteral {
                    literal: number_literal.clone(),
                    context: evaluation_context.to_owned(),
                })?;

            let number_value = serde_json::Number::from_f64(parsed_number).ok_or_else(|| WorkflowRuntimeError::InvalidNumberLiteral {
                literal: normalized_literal,
                context: evaluation_context.to_owned(),
            })?;

            Ok(Value::Number(number_value))
        }
        Expression::BooleanLiteral(boolean_value) => Ok(Value::Bool(*boolean_value)),
        Expression::NullLiteral => Ok(Value::Null),
        Expression::Reference(reference) => evaluate_reference(reference, execution_scope, evaluation_context),
        Expression::ArrayLiteral(array_values) => {
            let mut rendered_values = Vec::new();

            for array_value in array_values {
                rendered_values.push(evaluate_expression(array_value, execution_scope, evaluation_context)?);
            }

            Ok(Value::Array(rendered_values))
        }
        Expression::ObjectLiteral(object_fields) => {
            let mut rendered_object = Map::new();

            for object_field in object_fields {
                let object_field_value = evaluate_expression(&object_field.value, execution_scope, evaluation_context)?;
                rendered_object.insert(object_field.name.clone(), object_field_value);
            }

            Ok(Value::Object(rendered_object))
        }
        Expression::FunctionCall(function_call) => {
            let function_name = reference_root_to_string(&function_call.callee.root);

            Err(WorkflowRuntimeError::UnsupportedFunctionCall {
                function_name,
                context: evaluation_context.to_owned(),
            })
        }
    }
}

pub(crate) fn render_value_as_text(value: &Value) -> String {
    match value {
        Value::String(string_value) => string_value.clone(),
        Value::Null => "null".to_owned(),
        Value::Bool(boolean_value) => boolean_value.to_string(),
        Value::Number(number_value) => number_value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

pub(crate) fn validate_value_against_type_expression(
    candidate_value: &Value,
    type_expression: &TypeExpression,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    match type_expression {
        TypeExpression::String => validate_primitive_match(candidate_value, current_path, "string", Value::is_string),
        TypeExpression::Number | TypeExpression::Float => {
            validate_primitive_match(candidate_value, current_path, "number", Value::is_number)
        }
        TypeExpression::Boolean => validate_primitive_match(candidate_value, current_path, "boolean", Value::is_boolean),
        TypeExpression::Null => validate_primitive_match(candidate_value, current_path, "null", Value::is_null),
        TypeExpression::SchemaReference(schema_name) => {
            validate_schema_reference_type(candidate_value, schema_name, workflow, current_path)
        }
        TypeExpression::StringEnum(enum_value) => validate_string_enum_type(candidate_value, enum_value, current_path),
        TypeExpression::Array { item_type, fixed_length } => {
            validate_array_type(candidate_value, item_type, *fixed_length, workflow, current_path)
        }
        TypeExpression::Tuple(tuple_types) => validate_tuple_type(candidate_value, tuple_types, workflow, current_path),
        TypeExpression::Object(object_fields) => validate_object_fields(candidate_value, object_fields.as_slice(), workflow, current_path),
        TypeExpression::Union(union_types) => validate_union_type(candidate_value, union_types, workflow, current_path),
    }
}

fn validate_primitive_match(
    candidate_value: &Value,
    current_path: &str,
    expected_type: &str,
    matcher: fn(&Value) -> bool,
) -> Result<(), String> {
    if matcher(candidate_value) {
        Ok(())
    } else {
        Err(format!(
            "expected {expected_type} at {current_path}, got {}",
            describe_json_value(candidate_value)
        ))
    }
}

fn validate_schema_reference_type(
    candidate_value: &Value,
    schema_name: &str,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let schema_declaration = workflow
        .find_schema(schema_name)
        .ok_or_else(|| format!("unknown schema reference '{schema_name}' at {current_path}"))?;

    validate_object_fields(candidate_value, schema_declaration.fields.as_slice(), workflow, current_path)
}

fn validate_string_enum_type(candidate_value: &Value, enum_value: &str, current_path: &str) -> Result<(), String> {
    match candidate_value.as_str() {
        Some(candidate_string) if candidate_string == enum_value => Ok(()),
        Some(candidate_string) => Err(format!("expected '{enum_value}' at {current_path}, got '{candidate_string}'")),
        None => Err(format!(
            "expected string enum '{enum_value}' at {current_path}, got {}",
            describe_json_value(candidate_value)
        )),
    }
}

fn validate_array_type(
    candidate_value: &Value,
    item_type: &TypeExpression,
    fixed_length: Option<u64>,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_array = candidate_value
        .as_array()
        .ok_or_else(|| format!("expected array at {current_path}, got {}", describe_json_value(candidate_value)))?;

    if let Some(expected_length) = fixed_length {
        let expected_length_as_usize =
            usize::try_from(expected_length).map_err(|_| format!("array length is too large at {current_path}"))?;

        if candidate_array.len() != expected_length_as_usize {
            return Err(format!(
                "expected array length {expected_length_as_usize} at {current_path}, got {}",
                candidate_array.len()
            ));
        }
    }

    for (index, array_item) in candidate_array.iter().enumerate() {
        let item_path = format!("{current_path}[{index}]");
        validate_value_against_type_expression(array_item, item_type, workflow, item_path.as_str())?;
    }

    Ok(())
}

fn validate_tuple_type(
    candidate_value: &Value,
    tuple_types: &[TypeExpression],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_array = candidate_value.as_array().ok_or_else(|| {
        format!(
            "expected tuple array at {current_path}, got {}",
            describe_json_value(candidate_value)
        )
    })?;

    if candidate_array.len() != tuple_types.len() {
        return Err(format!(
            "expected tuple length {} at {current_path}, got {}",
            tuple_types.len(),
            candidate_array.len()
        ));
    }

    for (index, (array_item, tuple_item_type)) in candidate_array.iter().zip(tuple_types).enumerate() {
        let item_path = format!("{current_path}[{index}]");
        validate_value_against_type_expression(array_item, tuple_item_type, workflow, item_path.as_str())?;
    }

    Ok(())
}

fn validate_union_type(
    candidate_value: &Value,
    union_types: &[TypeExpression],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let mut mismatch_messages = Vec::new();

    for union_type in union_types {
        match validate_value_against_type_expression(candidate_value, union_type, workflow, current_path) {
            Ok(()) => {
                return Ok(());
            }
            Err(mismatch_message) => {
                mismatch_messages.push(mismatch_message);
            }
        }
    }

    Err(format!(
        "value at {current_path} does not match any union type: {}",
        mismatch_messages.join(" | ")
    ))
}

fn validate_object_fields(
    candidate_value: &Value,
    object_fields: &[TypedField],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_object = candidate_value
        .as_object()
        .ok_or_else(|| format!("expected object at {current_path}, got {}", describe_json_value(candidate_value)))?;

    for object_field in object_fields {
        let object_field_value = candidate_object
            .get(object_field.name.as_str())
            .ok_or_else(|| format!("missing required field '{}' at {current_path}", object_field.name))?;

        let field_path = format!("{current_path}.{}", object_field.name);

        validate_value_against_type_expression(object_field_value, &object_field.field_type, workflow, field_path.as_str())?;
    }

    Ok(())
}

fn describe_json_value(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn evaluate_reference(
    reference: &Reference,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    match &reference.root {
        ReferenceRoot::Keyword(ReferenceKeyword::Agent) => evaluate_agent_reference(reference, execution_scope, evaluation_context),
        ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
            apply_reference_accesses(reference, execution_scope.input_values, 0, evaluation_context)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
            apply_reference_accesses(reference, execution_scope.secret_values, 0, evaluation_context)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Tool) => Err(WorkflowRuntimeError::UnsupportedToolKeywordReference {
            context: evaluation_context.to_owned(),
        }),
        ReferenceRoot::Identifier(identifier) => Err(WorkflowRuntimeError::UnknownReferenceIdentifier {
            identifier: identifier.clone(),
            context: evaluation_context.to_owned(),
        }),
    }
}

fn evaluate_agent_reference(
    reference: &Reference,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let Some(first_access) = reference.accesses.first() else {
        return Err(WorkflowRuntimeError::InvalidReferencePath {
            reference_path: reference_to_string(reference),
            field_name: "<missing-agent-name>".to_owned(),
            context: evaluation_context.to_owned(),
        });
    };

    let starting_value = execution_scope
        .agent_outputs_by_name
        .get(first_access.field.as_str())
        .ok_or_else(|| WorkflowRuntimeError::InvalidReferencePath {
            reference_path: reference_to_string(reference),
            field_name: first_access.field.clone(),
            context: evaluation_context.to_owned(),
        })?;

    if reference.accesses.len() == 1 {
        return Ok(starting_value.clone());
    }

    apply_reference_accesses(reference, starting_value, 1, evaluation_context)
}

fn apply_reference_accesses(
    reference: &Reference,
    starting_value: &Value,
    start_index: usize,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let mut current_value = starting_value.clone();

    for reference_access in reference.accesses.iter().skip(start_index) {
        match &current_value {
            Value::Object(object_value) => {
                if let Some(next_value) = object_value.get(reference_access.field.as_str()) {
                    current_value = next_value.clone();
                } else if reference_access.optional {
                    return Ok(Value::Null);
                } else {
                    return Err(WorkflowRuntimeError::InvalidReferencePath {
                        reference_path: reference_to_string(reference),
                        field_name: reference_access.field.clone(),
                        context: evaluation_context.to_owned(),
                    });
                }
            }
            _ if reference_access.optional => {
                return Ok(Value::Null);
            }
            _ => {
                return Err(WorkflowRuntimeError::InvalidReferencePath {
                    reference_path: reference_to_string(reference),
                    field_name: reference_access.field.clone(),
                    context: evaluation_context.to_owned(),
                });
            }
        }
    }

    Ok(current_value)
}

fn reference_root_to_string(reference_root: &ReferenceRoot) -> String {
    match reference_root {
        ReferenceRoot::Keyword(reference_keyword) => reference_keyword.as_str().to_owned(),
        ReferenceRoot::Identifier(identifier) => identifier.clone(),
    }
}

fn reference_to_string(reference: &Reference) -> String {
    let mut rendered_reference = reference_root_to_string(&reference.root);

    for reference_access in &reference.accesses {
        let access_operator = if reference_access.optional { "?." } else { "." };

        rendered_reference.push_str(access_operator);
        rendered_reference.push_str(reference_access.field.as_str());
    }

    rendered_reference
}
