use crate::diagnostics::CommandError;
use engine_ai_core::runtime::types::{validate_value_against_type, WorkflowType};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const SECRET_ENVIRONMENT_PREFIX: &str = "ENGINE_AI_SECRET_";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretAssignment {
    pub name: String,
    pub value: String,
}

impl SecretAssignment {
    pub fn parse(raw_secret_assignment: &str) -> Result<Self, CommandError> {
        let maybe_assignment_parts = raw_secret_assignment.split_once('=');

        let Some((raw_name, raw_value)) = maybe_assignment_parts else {
            return Err(CommandError::invalid_workflow("secret must use NAME=VALUE format"));
        };

        let secret_name = raw_name.trim().to_owned();
        let secret_value = raw_value.trim().to_owned();

        if secret_name.is_empty() {
            return Err(CommandError::invalid_workflow("secret name cannot be empty"));
        }

        Ok(Self {
            name: secret_name,
            value: secret_value,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowInvocationBindings {
    pub input_values: Value,
    pub secret_values: Map<String, Value>,
}

pub fn parse_workflow_invocation_bindings(
    raw_arguments: &[String],
    input_type: Option<&WorkflowType>,
    secrets_type: Option<&WorkflowType>,
) -> Result<WorkflowInvocationBindings, CommandError> {
    let parsed_invocation_arguments = ParsedInvocationArguments::parse(raw_arguments)?;
    let input_values = parsed_invocation_arguments.resolve_input_values(input_type)?;
    let secret_values = parsed_invocation_arguments.resolve_secret_values(secrets_type)?;

    Ok(WorkflowInvocationBindings {
        input_values,
        secret_values,
    })
}

#[derive(Debug, Clone)]
struct ParsedInvocationArguments {
    raw_input_assignments: BTreeMap<String, Option<String>>,
    raw_secret_assignments: Vec<SecretAssignment>,
}

impl ParsedInvocationArguments {
    fn parse(raw_arguments: &[String]) -> Result<Self, CommandError> {
        let mut raw_input_assignments = BTreeMap::new();
        let mut raw_secret_assignments = Vec::new();
        let mut argument_index = 0;

        while argument_index < raw_arguments.len() {
            let current_argument = &raw_arguments[argument_index];

            if let Some(secret_assignment) = current_argument.strip_prefix("--secret=") {
                raw_secret_assignments.push(SecretAssignment::parse(secret_assignment)?);
                argument_index += 1;

                continue;
            }

            if current_argument == "--secret" {
                let Some(raw_secret_assignment) = raw_arguments.get(argument_index + 1) else {
                    return Err(CommandError::invalid_workflow("missing value for `--secret`"));
                };

                raw_secret_assignments.push(SecretAssignment::parse(raw_secret_assignment)?);
                argument_index += 2;

                continue;
            }

            let Some(raw_flag_body) = current_argument.strip_prefix("--") else {
                return Err(CommandError::invalid_workflow(format!(
                    "unexpected positional argument `{current_argument}`"
                )));
            };

            if raw_flag_body.is_empty() {
                return Err(CommandError::invalid_workflow("invalid empty flag `--`"));
            }

            if let Some((field_name, field_value)) = raw_flag_body.split_once('=') {
                raw_input_assignments.insert(field_name.to_string(), Some(field_value.to_string()));
                argument_index += 1;

                continue;
            }

            let maybe_following_value = raw_arguments
                .get(argument_index + 1)
                .filter(|following_value| !following_value.starts_with("--"))
                .cloned();

            if maybe_following_value.is_some() {
                argument_index += 2;
            } else {
                argument_index += 1;
            }

            raw_input_assignments.insert(raw_flag_body.to_string(), maybe_following_value);
        }

        Ok(Self {
            raw_input_assignments,
            raw_secret_assignments,
        })
    }

    fn resolve_input_values(&self, input_type: Option<&WorkflowType>) -> Result<Value, CommandError> {
        let Some(input_type) = input_type else {
            if self.raw_input_assignments.is_empty() {
                return Ok(Value::Object(Map::new()));
            }

            return Err(CommandError::invalid_workflow("workflow does not declare an `input` block"));
        };

        let WorkflowType::Object(declared_fields) = input_type else {
            return Err(CommandError::internal("workflow input type must be an object"));
        };

        let mut resolved_input_values = Map::new();

        for (field_name, maybe_raw_field_value) in &self.raw_input_assignments {
            let Some(field_type) = declared_fields.get(field_name) else {
                return Err(CommandError::invalid_workflow(format!(
                    "unknown workflow input field `--{field_name}`"
                )));
            };

            let field_value = field_type
                .parse_cli_argument_value(maybe_raw_field_value.as_deref(), field_name)
                .map_err(CommandError::invalid_workflow)?;

            resolved_input_values.insert(field_name.clone(), field_value);
        }

        let input_value = Value::Object(resolved_input_values);

        validate_value_against_type(&input_value, input_type)
            .map_err(|message| CommandError::invalid_workflow(format!("workflow input value is invalid: {message}")))?;

        Ok(input_value)
    }

    fn resolve_secret_values(&self, secrets_type: Option<&WorkflowType>) -> Result<Map<String, Value>, CommandError> {
        let Some(secrets_type) = secrets_type else {
            if self.raw_secret_assignments.is_empty() {
                return Ok(Map::new());
            }

            return Err(CommandError::invalid_workflow("workflow does not declare a `secrets` block"));
        };

        let WorkflowType::Object(declared_fields) = secrets_type else {
            return Err(CommandError::internal("workflow secrets type must be an object"));
        };

        let mut raw_secret_values = BTreeMap::new();

        for (environment_name, environment_value) in std::env::vars() {
            let Some(raw_secret_name) = environment_name.strip_prefix(SECRET_ENVIRONMENT_PREFIX) else {
                continue;
            };

            let Some(normalized_secret_name) = resolve_declared_secret_name(raw_secret_name, declared_fields) else {
                continue;
            };

            raw_secret_values.insert(normalized_secret_name, environment_value);
        }

        for secret_assignment in &self.raw_secret_assignments {
            let normalized_secret_name = resolve_declared_secret_name(&secret_assignment.name, declared_fields)
                .ok_or_else(|| CommandError::invalid_workflow(format!("unknown workflow secret `{}`", secret_assignment.name)))?;

            raw_secret_values.insert(normalized_secret_name, secret_assignment.value.clone());
        }

        let mut resolved_secret_values = Map::new();

        for (secret_name, raw_secret_value) in raw_secret_values {
            let secret_type = declared_fields
                .get(&secret_name)
                .expect("resolved secret name must exist in declared secrets");

            let secret_value = secret_type
                .parse_cli_argument_value(Some(&raw_secret_value), &secret_name)
                .map_err(CommandError::invalid_workflow)?;

            resolved_secret_values.insert(secret_name, secret_value);
        }

        let secret_value = Value::Object(resolved_secret_values.clone());

        validate_value_against_type(&secret_value, secrets_type)
            .map_err(|message| CommandError::invalid_workflow(format!("workflow secrets value is invalid: {message}")))?;

        Ok(resolved_secret_values)
    }
}

fn resolve_declared_secret_name(requested_secret_name: &str, declared_fields: &BTreeMap<String, WorkflowType>) -> Option<String> {
    if declared_fields.contains_key(requested_secret_name) {
        return Some(requested_secret_name.to_string());
    }

    declared_fields
        .keys()
        .find(|declared_secret_name| declared_secret_name.eq_ignore_ascii_case(requested_secret_name))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::{parse_workflow_invocation_bindings, SecretAssignment};
    use engine_ai_core::runtime::types::WorkflowType;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    #[test]
    fn parses_secret_assignment_name_and_value() {
        let parsed_secret_assignment = SecretAssignment::parse("api_key=test-value").expect("secret assignment should parse");

        assert_eq!(
            parsed_secret_assignment,
            SecretAssignment {
                name: "api_key".to_string(),
                value: "test-value".to_string(),
            }
        );
    }

    #[test]
    fn parses_scalar_and_json_input_flags() {
        let mut input_fields = BTreeMap::new();
        input_fields.insert("topic".to_string(), WorkflowType::String);
        input_fields.insert(
            "items".to_string(),
            WorkflowType::Array {
                item_type: Box::new(WorkflowType::String),
                fixed_length: None,
            },
        );
        input_fields.insert("enabled".to_string(), WorkflowType::Boolean);

        let input_type = WorkflowType::Object(input_fields);
        let raw_arguments = vec![
            "--topic=Hello".to_string(),
            "--items".to_string(),
            "[\"a\",\"b\"]".to_string(),
            "--enabled".to_string(),
        ];
        let parsed_bindings =
            parse_workflow_invocation_bindings(&raw_arguments, Some(&input_type), None).expect("workflow input arguments should parse");

        assert_eq!(
            parsed_bindings.input_values,
            json!({
                "topic": "Hello",
                "items": ["a", "b"],
                "enabled": true,
            })
        );
    }

    #[test]
    fn resolves_secrets_from_environment_and_flags_with_flag_precedence() {
        let mut secrets_fields = BTreeMap::new();
        secrets_fields.insert("api_key".to_string(), WorkflowType::String);
        let secrets_type = WorkflowType::Object(secrets_fields);

        unsafe {
            std::env::set_var("ENGINE_AI_SECRET_API_KEY", "from-env");
        }

        let raw_arguments = vec!["--secret".to_string(), "api_key=from-cli".to_string()];
        let parsed_bindings =
            parse_workflow_invocation_bindings(&raw_arguments, None, Some(&secrets_type)).expect("workflow secret arguments should parse");

        assert_eq!(
            parsed_bindings.secret_values.get("api_key"),
            Some(&Value::String("from-cli".to_string()))
        );

        unsafe {
            std::env::remove_var("ENGINE_AI_SECRET_API_KEY");
        }
    }

    #[test]
    fn rejects_unknown_input_field() {
        let mut input_fields = BTreeMap::new();
        input_fields.insert("topic".to_string(), WorkflowType::String);
        let input_type = WorkflowType::Object(input_fields);
        let raw_arguments = vec!["--missing=value".to_string()];
        let parse_result = parse_workflow_invocation_bindings(&raw_arguments, Some(&input_type), None);

        assert!(parse_result.is_err());
    }
}
