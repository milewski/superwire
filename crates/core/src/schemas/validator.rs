use crate::schemas::error::SchemaError;
use jsonschema::Validator;
use serde_json::Value;

pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(schema: &Value, data: &Value) -> Result<(), SchemaError> {
        let compiled_schema = Validator::new(schema).map_err(|error| SchemaError::CompilationError {
            schema_name: None,
            message: format!("Failed to compile schema: {}", error),
            suggestion: Some("Check that the schema is valid JSON Schema".to_string()),
        })?;

        if let Err(error) = compiled_schema.validate(data) {
            let error_message = format!("{}: {}", error.instance_path, error);

            return Err(SchemaError::ValidationError {
                schema_name: None,
                field_path: Some(error.instance_path.to_string()),
                message: error_message,
                suggestion: Some("Ensure the data matches the schema structure".to_string()),
            });
        }

        Ok(())
    }

    pub fn inject_schema_into_prompt(schema: &Value) -> String {
        format!(
            "You must return your response as JSON following this exact schema:\n\n{}\n\nEnsure your output is valid JSON that matches this structure.",
            serde_json::to_string_pretty(schema).unwrap()
        )
    }
}
