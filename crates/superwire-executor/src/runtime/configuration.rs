use super::{ExecutorError, WorkflowExecutor};
use serde_json::{Map, Value};
use std::collections::HashSet;
use superwire_core::semantic::support::types::{validate_value_against_type, value_kind_name, WorkflowType};
use superwire_dsl::{Expression, ReferenceKeyword};

#[derive(Debug, Clone, Copy)]
pub(in crate::runtime) struct RuntimeValidationContext<'a> {
    pub(in crate::runtime) input: &'a Value,
    pub(in crate::runtime) secrets: &'a Value,
}

pub(in crate::runtime) struct ResolvedRuntimeConfiguration {
    pub(in crate::runtime) input_values: Map<String, Value>,
    pub(in crate::runtime) secret_values: Map<String, Value>,
}

impl WorkflowExecutor {
    pub fn validate_runtime_configuration(&self, input: &Value, secrets: &Value) -> Result<(), ExecutorError> {
        self.resolve_runtime_configuration(RuntimeValidationContext { input, secrets })?;

        Ok(())
    }

    pub fn validate_runtime_configuration_without_input(&self, secrets: &Value) -> Result<(), ExecutorError> {
        self.resolve_secret_values(secrets)?;

        Ok(())
    }

    pub(in crate::runtime) fn resolve_runtime_configuration(
        &self,
        validation_context: RuntimeValidationContext<'_>,
    ) -> Result<ResolvedRuntimeConfiguration, ExecutorError> {
        let input_values = self.resolve_input_values(validation_context.input)?;
        let secret_values = self.resolve_secret_values(validation_context.secrets)?;

        Ok(ResolvedRuntimeConfiguration {
            input_values,
            secret_values,
        })
    }

    pub(super) fn resolve_input_values(&self, input: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(input_type) = &self.execution_plan.input_type {
            if input.is_null() {
                if let WorkflowType::Object(field_types) = input_type {
                    let tool_consumed_fields = self.input_fields_consumed_by_bindings();

                    if field_types.keys().all(|field_name| tool_consumed_fields.contains(field_name)) {
                        let input_map = field_types
                            .keys()
                            .map(|field_name| (field_name.clone(), Value::Null))
                            .collect::<Map<String, Value>>();

                        return Ok(input_map);
                    }

                    let uncovered_fields = field_types
                        .keys()
                        .filter(|field_name| !tool_consumed_fields.contains(field_name.as_str()))
                        .cloned()
                        .collect::<Vec<_>>();

                    return Err(ExecutorError::InputValueMismatch {
                        message: format!(
                            "workflow declares an `input` block, but no input object was provided; \
                             the following fields are not covered by tool bindings and must be provided: {}",
                            uncovered_fields.join(", ")
                        ),
                    });
                }

                return Err(ExecutorError::InputValueMismatch {
                    message: format!("workflow declares an `input` block, but no input object was provided; expected {input_type}"),
                });
            }

            validate_value_against_type(input, input_type).map_err(|message| ExecutorError::InputValueMismatch {
                message: format!("declared `input` block expects {input_type}: {message}"),
            })?;

            return input.as_object().cloned().ok_or_else(|| ExecutorError::InputValueMismatch {
                message: format!(
                    "declared `input` block expects object matching {input_type}, found {}",
                    value_kind_name(input)
                ),
            });
        }

        if input.is_null() || input.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: value_kind_name(input).to_string(),
        })
    }

    pub(super) fn resolve_secret_values(&self, secrets: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(secrets_type) = &self.execution_plan.secrets_type {
            if secrets.is_null() {
                return Err(ExecutorError::SecretValueMismatch {
                    message: format!("workflow declares a `secrets` block, but no secrets object was provided; expected {secrets_type}"),
                });
            }

            validate_value_against_type(secrets, secrets_type).map_err(|message| ExecutorError::SecretValueMismatch {
                message: format!("declared `secrets` block expects {secrets_type}: {message}"),
            })?;

            return secrets.as_object().cloned().ok_or_else(|| ExecutorError::SecretValueMismatch {
                message: format!(
                    "declared `secrets` block expects object matching {secrets_type}, found {}",
                    value_kind_name(secrets)
                ),
            });
        }

        if secrets.is_null() || secrets.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no secrets".to_string(),
            found: value_kind_name(secrets).to_string(),
        })
    }

    fn input_fields_consumed_by_bindings(&self) -> HashSet<String> {
        let mut consumed_fields = HashSet::new();

        for tool in self.execution_plan.tools.values() {
            for fixed_binding in &tool.declaration.fixed_binding_fields {
                if let Expression::Reference(reference) = &fixed_binding.value {
                    if let Some(field_name) = reference.direct_required_name_for_keyword(ReferenceKeyword::Input) {
                        consumed_fields.insert(field_name.to_string());
                    }
                }
            }
        }

        consumed_fields
    }
}
