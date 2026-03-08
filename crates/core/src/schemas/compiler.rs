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

            let mut field_schema_obj = field_schema.as_object().unwrap().clone();

            if let Some(description) = &field.description {
                field_schema_obj.insert("description".to_string(), Value::String(description.clone()));
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

                    let mut field_schema_obj = field_schema.as_object().unwrap().clone();

                    if let Some(description) = &field.description {
                        field_schema_obj.insert("description".to_string(), Value::String(description.clone()));
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
