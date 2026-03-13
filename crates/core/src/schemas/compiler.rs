use crate::ast::{Schema, SchemaType};
use crate::schemas::error::SchemaError;
use serde_json::Value;
use std::collections::HashMap;

pub struct SchemaCompiler;

impl SchemaCompiler {
    pub fn compile(schema: &Schema) -> Result<Value, SchemaError> {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for field in &schema.fields {
            let field_schema = Self::compile_field_type(&field.field_type)?;

            let mut field_schema_obj = match field_schema.as_object() {
                Some(obj) => obj.clone(),
                None => {
                    return Err(SchemaError::CompilationError {
                        schema_name: Some(field.name.clone()),
                        message: format!("Field '{}' schema is not a valid object", field.name),
                        suggestion: None,
                    });
                }
            };

            if let Some(description) = &field.description {
                field_schema_obj.insert("description".to_string(), Value::String(description.to_string()));
            }

            properties.insert(field.name.clone(), Value::Object(field_schema_obj));

            if !Self::is_nullable(&field.field_type) {
                required.push(field.name.clone());
            }
        }

        let schema_obj = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        });

        Ok(schema_obj)
    }

    pub fn compile_type(schema_type: &SchemaType, description: Option<&str>) -> Result<Value, SchemaError> {
        let mut type_schema = Self::compile_field_type(schema_type)?;

        if let Some(description_text) = description {
            if let Value::Object(ref mut obj) = type_schema {
                obj.insert("description".to_string(), Value::String(description_text.to_string()));
            }
        }

        Ok(type_schema)
    }

    fn compile_field_type(field_type: &SchemaType) -> Result<Value, SchemaError> {
        match field_type {
            SchemaType::String => Ok(serde_json::json!({"type": "string"})),

            SchemaType::Number => Ok(serde_json::json!({"type": "number"})),

            SchemaType::Boolean => Ok(serde_json::json!({"type": "boolean"})),

            SchemaType::Null => Ok(serde_json::json!({"type": "null"})),

            SchemaType::Array(inner) => {
                let inner_schema = Self::compile_field_type(inner)?;
                Ok(serde_json::json!({
                    "type": "array",
                    "items": inner_schema
                }))
            }

            SchemaType::Enum(variants) => {
                let is_type_union = variants
                    .iter()
                    .all(|v| matches!(v.as_str(), "string" | "number" | "boolean" | "null"));

                if is_type_union {
                    let types: Vec<Value> = variants.iter().map(|v| Value::String(v.clone())).collect();

                    Ok(serde_json::json!({
                        "type": types
                    }))
                } else {
                    let enum_values: Vec<Value> =
                        variants.iter().map(|variant| Value::String(variant.clone())).collect();

                    Ok(serde_json::json!({
                        "enum": enum_values
                    }))
                }
            }

            SchemaType::Object(fields) => {
                let mut properties = HashMap::new();
                let mut required = Vec::new();

                for field in fields {
                    let field_schema = Self::compile_field_type(&field.field_type)?;

                    let mut field_schema_obj = match field_schema.as_object() {
                        Some(obj) => obj.clone(),
                        None => {
                            return Err(SchemaError::CompilationError {
                                schema_name: Some(field.name.clone()),
                                message: format!("Field '{}' schema is not a valid object", field.name),
                                suggestion: None,
                            });
                        }
                    };

                    if let Some(description) = &field.description {
                        field_schema_obj.insert("description".to_string(), Value::String(description.to_string()));
                    }

                    properties.insert(field.name.clone(), Value::Object(field_schema_obj));

                    if !Self::is_nullable(&field.field_type) {
                        required.push(field.name.clone());
                    }
                }

                Ok(serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }))
            }
        }
    }

    fn is_nullable(field_type: &SchemaType) -> bool {
        match field_type {
            SchemaType::Null => true,
            SchemaType::Enum(variants) => variants.contains(&"null".to_string()),
            _ => false,
        }
    }
}
