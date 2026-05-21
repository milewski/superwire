use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::diagnostics::CommandError;

pub(super) struct WorkflowPayloadSources<'source> {
    input_json: Option<&'source str>,
    input_file: Option<&'source Path>,
    secrets_json: Option<&'source str>,
    secrets_file: Option<&'source Path>,
    set_arguments: Option<&'source [String]>,
}

impl<'source> WorkflowPayloadSources<'source> {
    pub(super) const fn new(
        input_json: Option<&'source str>,
        input_file: Option<&'source Path>,
        secrets_json: Option<&'source str>,
        secrets_file: Option<&'source Path>,
        set_arguments: Option<&'source [String]>,
    ) -> Self {
        Self {
            input_json,
            input_file,
            secrets_json,
            secrets_file,
            set_arguments,
        }
    }

    pub(super) fn validate(&self) -> Result<(), CommandError> {
        if self.input_json.is_some() && self.input_file.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --input-file, not both"));
        }

        if self.input_json.is_some() && self.set_arguments.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --set, not both"));
        }

        if self.input_file.is_some() && self.set_arguments.is_some() {
            return Err(CommandError::invalid_input("use either --input-file or --set, not both"));
        }

        if self.secrets_json.is_some() && self.secrets_file.is_some() {
            return Err(CommandError::invalid_input("use either --secrets-json or --secrets-file, not both"));
        }

        Ok(())
    }

    pub(super) fn input_value(&self) -> Result<Map<String, Value>, CommandError> {
        let base_payload = Self::payload_as_object(self.input_json, self.input_file, "input payload")?;

        self.apply_dot_params(base_payload)
    }

    pub(super) fn secrets_value(&self) -> Result<Map<String, Value>, CommandError> {
        Self::payload_as_object(self.secrets_json, self.secrets_file, "secrets payload")
    }

    fn apply_dot_params(&self, mut payload: Map<String, Value>) -> Result<Map<String, Value>, CommandError> {
        let Some(set_arguments) = self.set_arguments else {
            return Ok(payload);
        };

        for key_value_pair in set_arguments {
            let Some((key, value)) = key_value_pair.split_once('=') else {
                return Err(CommandError::invalid_input(format!(
                    "invalid --set format: expected KEY=VALUE, got '{key_value_pair}'"
                )));
            };

            Self::insert_dot_parameter(&mut payload, key.trim(), value.trim())?;
        }

        Ok(payload)
    }

    fn insert_dot_parameter(payload: &mut Map<String, Value>, key: &str, value: &str) -> Result<(), CommandError> {
        let mut current_payload = payload;
        let key_parts = key.split('.').collect::<Vec<_>>();

        for (key_part_index, key_part) in key_parts.iter().enumerate() {
            let is_last_key_part = key_part_index == key_parts.len() - 1;

            if is_last_key_part {
                current_payload.insert((*key_part).to_string(), Value::String(value.to_string()));
            } else {
                if !current_payload.contains_key(*key_part) {
                    current_payload.insert((*key_part).to_string(), Value::Object(Map::new()));
                }

                let Some(object_payload) = current_payload.get_mut(*key_part).and_then(Value::as_object_mut) else {
                    return Err(CommandError::invalid_input(format!(
                        "cannot set nested value on non-object path: {key}"
                    )));
                };

                current_payload = object_payload;
            }
        }

        Ok(())
    }

    fn payload_as_object(
        inline_payload: Option<&str>,
        payload_file_path: Option<&Path>,
        payload_label: &str,
    ) -> Result<Map<String, Value>, CommandError> {
        let payload_json = if let Some(inline_payload) = inline_payload {
            inline_payload.to_string()
        } else if let Some(payload_file_path) = payload_file_path {
            fs::read_to_string(payload_file_path).map_err(|read_error| {
                CommandError::invalid_input(format!(
                    "failed to read {payload_label} file {}: {read_error}",
                    payload_file_path.display()
                ))
            })?
        } else {
            "{}".to_string()
        };

        let parsed_payload_value = serde_json::from_str::<Value>(&payload_json)
            .map_err(|parse_error| CommandError::invalid_input(format!("{payload_label} must be valid json: {parse_error}")))?;

        let Some(parsed_payload_object) = parsed_payload_value.as_object() else {
            return Err(CommandError::invalid_input(format!("{payload_label} must be a json object")));
        };

        Ok(parsed_payload_object.clone())
    }
}
