use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};

use serde_json::{Map, Value};
use superwire_dsl::{TypeExpression, TypedField, Workflow};
use superwire_mcp::McpLockResolutionContext;

use crate::diagnostics::CommandError;

pub(super) struct WorkflowLockPrompts;

impl WorkflowLockPrompts {
    pub(super) fn resolve_lock_context(
        parsed_workflow: &Workflow,
        lock_context: &mut McpLockResolutionContext,
    ) -> Result<PromptedLockContext, CommandError> {
        let mut prompted_value_was_captured = false;

        if let Some(input_declaration) = parsed_workflow.find_input() {
            if Self::prompt_for_missing_fields(
                parsed_workflow,
                PromptSection::Input,
                &input_declaration.fields,
                &mut lock_context.input,
            )? {
                prompted_value_was_captured = true;
            }
        }

        if let Some(secrets_declaration) = parsed_workflow.find_secrets() {
            if Self::prompt_for_missing_fields(
                parsed_workflow,
                PromptSection::Secrets,
                &secrets_declaration.fields,
                &mut lock_context.secrets,
            )? {
                prompted_value_was_captured = true;
            }
        }

        Ok(PromptedLockContext {
            lock_context: lock_context.clone(),
            prompted_value_was_captured,
        })
    }

    fn prompt_for_missing_fields(
        parsed_workflow: &Workflow,
        prompt_section: PromptSection,
        typed_fields: &[TypedField],
        existing_values: &mut BTreeMap<String, Value>,
    ) -> Result<bool, CommandError> {
        let mut prompted_value_was_captured = false;

        for typed_field in typed_fields {
            let field_path = typed_field.name.clone();
            let existing_value = existing_values.remove(&typed_field.name);
            let (field_value, field_value_was_prompted) = Self::prompt_for_missing_value(
                parsed_workflow,
                prompt_section,
                &field_path,
                &typed_field.field_type,
                existing_value,
            )?;

            existing_values.insert(typed_field.name.clone(), field_value);

            if field_value_was_prompted {
                prompted_value_was_captured = true;
            }
        }

        Ok(prompted_value_was_captured)
    }

    fn prompt_for_missing_value(
        parsed_workflow: &Workflow,
        prompt_section: PromptSection,
        field_path: &str,
        type_expression: &TypeExpression,
        existing_value: Option<Value>,
    ) -> Result<(Value, bool), CommandError> {
        if let Some(object_fields) = Self::object_fields_for_prompt(parsed_workflow, type_expression) {
            let mut object_value = match existing_value {
                Some(Value::Object(object_value)) => object_value,
                Some(existing_value) => {
                    return Err(CommandError::invalid_input(format!(
                        "invalid value for {}.{field_path}: expected object, got {}",
                        prompt_section.as_str(),
                        Self::json_value_type_label(&existing_value)
                    )));
                }
                None => Map::new(),
            };
            let mut prompted_value_was_captured = false;

            for object_field in object_fields {
                let child_field_path = format!("{field_path}.{}", object_field.name);
                let existing_child_value = object_value.remove(&object_field.name);
                let (child_value, child_value_was_prompted) = Self::prompt_for_missing_value(
                    parsed_workflow,
                    prompt_section,
                    &child_field_path,
                    &object_field.field_type,
                    existing_child_value,
                )?;

                object_value.insert(object_field.name.clone(), child_value);

                if child_value_was_prompted {
                    prompted_value_was_captured = true;
                }
            }

            return Ok((Value::Object(object_value), prompted_value_was_captured));
        }

        if let Some(existing_value) = existing_value {
            return Ok((existing_value, false));
        }

        let field_value = Self::prompt_for_field_value(prompt_section, field_path, type_expression)?;

        Ok((field_value, true))
    }

    fn object_fields_for_prompt<'workflow>(
        parsed_workflow: &'workflow Workflow,
        type_expression: &'workflow TypeExpression,
    ) -> Option<&'workflow [TypedField]> {
        match type_expression {
            TypeExpression::Object(object_fields) => Some(object_fields),
            TypeExpression::SchemaReference(schema_name) => {
                let schema_declaration = parsed_workflow.find_schema(schema_name)?;

                if schema_declaration.root_variant.is_some() {
                    return None;
                }

                Some(schema_declaration.fields.as_slice())
            }
            TypeExpression::Union(type_expressions) => {
                let mut object_fields = None;

                for type_expression in type_expressions {
                    if matches!(type_expression, TypeExpression::Null) {
                        continue;
                    }

                    if object_fields.is_some() {
                        return None;
                    }

                    object_fields = Self::object_fields_for_prompt(parsed_workflow, type_expression);
                }

                object_fields
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
            | TypeExpression::Tuple(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    fn prompt_for_field_value(
        prompt_section: PromptSection,
        field_path: &str,
        type_expression: &TypeExpression,
    ) -> Result<Value, CommandError> {
        if !io::stdin().is_terminal() {
            return Err(CommandError::invalid_input(format!(
                "missing {}.{field_path} and terminal is non-interactive; provide it via .wire.vars, --vars-file, --input-json, --secrets-json, or --set",
                prompt_section.as_str()
            )));
        }

        let type_expression_label = Self::type_expression_label(type_expression);
        let prompt_message = format!(
            "missing {}.{field_path} ({type_expression_label}) - enter value: ",
            prompt_section.as_str()
        );

        print!("{prompt_message}");

        io::stdout()
            .flush()
            .map_err(|flush_error| CommandError::internal(format!("failed to flush prompt output: {flush_error}")))?;

        let mut input_buffer = String::new();

        io::stdin()
            .read_line(&mut input_buffer)
            .map_err(|read_error| CommandError::internal(format!("failed to read prompt input: {read_error}")))?;

        let trimmed_input = input_buffer.trim();

        if trimmed_input.is_empty() {
            return Err(CommandError::invalid_input(format!(
                "missing {}.{field_path}; empty value is not allowed",
                prompt_section.as_str()
            )));
        }

        Self::parse_prompt_value(trimmed_input, type_expression, prompt_section, field_path)
    }

    fn json_value_type_label(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    fn type_expression_label(type_expression: &TypeExpression) -> &'static str {
        match type_expression {
            TypeExpression::String | TypeExpression::StringEnum(_) | TypeExpression::StringEnumReference(_) => "string",
            TypeExpression::Number => "integer",
            TypeExpression::Float => "float",
            TypeExpression::Boolean => "boolean",
            TypeExpression::Null => "null",
            TypeExpression::AnyObject => "json",
            TypeExpression::SchemaReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::Union(_) => "json",
        }
    }

    fn parse_prompt_value(
        input_text: &str,
        type_expression: &TypeExpression,
        prompt_section: PromptSection,
        field_name: &str,
    ) -> Result<Value, CommandError> {
        match type_expression {
            TypeExpression::String | TypeExpression::StringEnum(_) | TypeExpression::StringEnumReference(_) => {
                Ok(Value::String(input_text.to_string()))
            }
            TypeExpression::Number => {
                let parsed_integer = input_text.parse::<i64>().map_err(|parse_error| {
                    CommandError::invalid_input(format!(
                        "invalid integer for {}.{field_name}: {parse_error}",
                        prompt_section.as_str()
                    ))
                })?;

                Ok(Value::Number(parsed_integer.into()))
            }
            TypeExpression::Float => {
                let parsed_float = input_text.parse::<f64>().map_err(|parse_error| {
                    CommandError::invalid_input(format!("invalid float for {}.{field_name}: {parse_error}", prompt_section.as_str()))
                })?;
                let Some(parsed_number) = serde_json::Number::from_f64(parsed_float) else {
                    return Err(CommandError::invalid_input(format!(
                        "invalid float for {}.{field_name}: value must be finite",
                        prompt_section.as_str()
                    )));
                };

                Ok(Value::Number(parsed_number))
            }
            TypeExpression::Boolean => {
                let parsed_boolean = input_text.parse::<bool>().map_err(|parse_error| {
                    CommandError::invalid_input(format!(
                        "invalid boolean for {}.{field_name}: {parse_error}",
                        prompt_section.as_str()
                    ))
                })?;

                Ok(Value::Bool(parsed_boolean))
            }
            TypeExpression::Null => {
                if input_text != "null" {
                    return Err(CommandError::invalid_input(format!(
                        "invalid null for {}.{field_name}: expected literal `null`",
                        prompt_section.as_str()
                    )));
                }

                Ok(Value::Null)
            }
            TypeExpression::AnyObject
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::SchemaReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Union(_) => serde_json::from_str::<Value>(input_text).map_err(|parse_error| {
                CommandError::invalid_input(format!("invalid json for {}.{field_name}: {parse_error}", prompt_section.as_str()))
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PromptSection {
    Input,
    Secrets,
}

impl PromptSection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Secrets => "secrets",
        }
    }
}

pub(super) struct PromptedLockContext {
    pub(super) lock_context: McpLockResolutionContext,
    pub(super) prompted_value_was_captured: bool,
}

impl PromptedLockContext {
    pub(super) fn as_ref(&self) -> Option<&McpLockResolutionContext> {
        if self.lock_context.input.is_empty()
            && self.lock_context.secrets.is_empty()
            && self.lock_context.dynamic.is_empty()
            && self.lock_context.agent_outputs.is_empty()
            && self.lock_context.agent_contexts.is_empty()
        {
            return None;
        }

        Some(&self.lock_context)
    }
}
