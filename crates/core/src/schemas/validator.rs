use crate::schemas::error::SchemaError;
use jsonschema::Validator;
use serde_json::Value;

pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(schema: &Value, data: &Value) -> Result<(), SchemaError> {
        log::debug!("Validating data against schema");
        log::trace!("Schema: {}", serde_json::to_string_pretty(schema).unwrap_or_default());
        log::trace!("Data: {}", serde_json::to_string_pretty(data).unwrap_or_default());

        let compiled_schema = Validator::new(schema).map_err(|error| {
            log::error!("Schema compilation failed: {error}");
            SchemaError::CompilationError {
                schema_name: None,
                message: format!("Failed to compile schema: {error}"),
                suggestion: Some("Check that the schema is valid JSON Schema".to_string()),
            }
        })?;

        if let Err(error) = compiled_schema.validate(data) {
            let error_message = format!("{}: {}", error.instance_path(), error);
            log::warn!("Schema validation failed: {error_message}");

            return Err(SchemaError::ValidationError {
                schema_name: None,
                field_path: Some(error.instance_path().to_string()),
                message: error_message,
                suggestion: Some("Ensure the data matches the schema structure".to_string()),
            });
        }

        Self::validate_no_empty_strings(schema, data)?;

        log::debug!("Schema validation successful");

        Ok(())
    }

    fn validate_no_empty_strings(schema: &Value, data: &Value) -> Result<(), SchemaError> {
        if let Some(schema_obj) = schema.as_object() {
            if let Some(required_fields) = schema_obj.get("required").and_then(|r| r.as_array()) {
                if let Some(data_obj) = data.as_object() {
                    for required_field in required_fields {
                        if let Some(field_name) = required_field.as_str() {
                            if let Some(field_value) = data_obj.get(field_name) {
                                if let Some(string_value) = field_value.as_str() {
                                    if string_value.is_empty() {
                                        log::warn!("Required field '{field_name}' is an empty string");
                                        return Err(SchemaError::ValidationError {
                                            schema_name: None,
                                            field_path: Some(format!("/{field_name}")),
                                            message: format!(
                                                "/{field_name}: Required field '{field_name}' cannot be an empty string"
                                            ),
                                            suggestion: Some(
                                                "Provide a non-empty value for this required field".to_string(),
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(properties) = schema_obj.get("properties").and_then(|p| p.as_object()) {
                if let Some(data_obj) = data.as_object() {
                    for (property_name, property_schema) in properties {
                        if let Some(property_data) = data_obj.get(property_name) {
                            if property_schema.is_object() {
                                Self::validate_no_empty_strings(property_schema, property_data)?;
                            }
                        }
                    }
                }
            }
        }

        if let Some(data_array) = data.as_array() {
            if let Some(schema_obj) = schema.as_object() {
                if let Some(items_schema) = schema_obj.get("items") {
                    for item in data_array {
                        Self::validate_no_empty_strings(items_schema, item)?;
                    }
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn inject_schema_into_prompt(schema: &Value) -> String {
        let pretty_schema = serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());

        let is_primitive_type = schema
            .as_object()
            .and_then(|obj| obj.get("type"))
            .and_then(|t| t.as_str())
            .is_some_and(|type_str| matches!(type_str, "string" | "number" | "integer" | "boolean" | "null"));

        let is_array_type = schema
            .as_object()
            .and_then(|obj| obj.get("type"))
            .and_then(|t| t.as_str())
            .is_some_and(|type_str| type_str == "array");

        if is_primitive_type {
            let type_name = schema
                .as_object()
                .and_then(|obj| obj.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("value");

            let description = schema
                .as_object()
                .and_then(|obj| obj.get("description"))
                .and_then(|d| d.as_str())
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();

            format!(
                "You must return your response as a {type_name} value{description}. Return ONLY the {type_name} value itself, not wrapped in an object or quotes (unless it's a string type)."
            )
        } else if is_array_type {
            format!(
                "You must return your response as a JSON array following this exact schema:\n\n{pretty_schema}\n\nIMPORTANT: Return the actual JSON array directly, NOT wrapped in an object."
            )
        } else {
            format!(
                "You must return your response as a JSON object (not a string) following this exact schema:\n\n{pretty_schema}\n\nIMPORTANT: Return the actual JSON object directly, NOT a string containing JSON. Do not escape quotes or wrap the JSON in quotes. Required string fields must not be empty strings - provide meaningful values."
            )
        }
    }
}
