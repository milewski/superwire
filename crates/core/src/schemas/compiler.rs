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

            SchemaType::Number => Ok(serde_json::json!({"type": "integer"})),

            SchemaType::Float => Ok(serde_json::json!({"type": "number"})),

            SchemaType::Boolean => Ok(serde_json::json!({"type": "boolean"})),

            SchemaType::Null => Ok(serde_json::json!({"type": "null"})),

            SchemaType::Array(inner, quantity) => {
                let inner_schema = Self::compile_field_type(inner)?;
                let mut array_schema = serde_json::json!({
                    "type": "array",
                    "items": inner_schema
                });

                if let Some(count) = quantity {
                    if let Some(obj) = array_schema.as_object_mut() {
                        obj.insert("minItems".to_string(), Value::Number((*count).into()));
                        obj.insert("maxItems".to_string(), Value::Number((*count).into()));
                    }
                }

                Ok(array_schema)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Schema, SchemaField, SchemaType, Span};

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0)
    }

    #[test]
    fn test_array_with_quantity_constraint() {
        let schema = Schema {
            fields: vec![SchemaField {
                name: "files".to_string(),
                field_type: SchemaType::Array(Box::new(SchemaType::String), Some(4)),
                description: Some("exactly 4 files".to_string()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };

        let compiled = SchemaCompiler::compile(&schema).unwrap();
        let properties = compiled.get("properties").unwrap().as_object().unwrap();
        let files_schema = properties.get("files").unwrap().as_object().unwrap();

        assert_eq!(files_schema.get("type").unwrap().as_str().unwrap(), "array");
        assert_eq!(files_schema.get("minItems").unwrap().as_u64().unwrap(), 4);
        assert_eq!(files_schema.get("maxItems").unwrap().as_u64().unwrap(), 4);
        assert_eq!(
            files_schema.get("description").unwrap().as_str().unwrap(),
            "exactly 4 files"
        );
    }

    #[test]
    fn test_array_without_quantity_constraint() {
        let schema = Schema {
            fields: vec![SchemaField {
                name: "items".to_string(),
                field_type: SchemaType::Array(Box::new(SchemaType::String), None),
                description: None,
                span: dummy_span(),
            }],
            span: dummy_span(),
        };

        let compiled = SchemaCompiler::compile(&schema).unwrap();
        let properties = compiled.get("properties").unwrap().as_object().unwrap();
        let items_schema = properties.get("items").unwrap().as_object().unwrap();

        assert_eq!(items_schema.get("type").unwrap().as_str().unwrap(), "array");
        assert!(items_schema.get("minItems").is_none());
        assert!(items_schema.get("maxItems").is_none());
    }

    #[test]
    fn test_nested_array_with_quantity() {
        let schema_type = SchemaType::Array(
            Box::new(SchemaType::Array(Box::new(SchemaType::Number), Some(3))),
            Some(2),
        );

        let compiled = SchemaCompiler::compile_type(&schema_type, None).unwrap();
        let obj = compiled.as_object().unwrap();

        assert_eq!(obj.get("type").unwrap().as_str().unwrap(), "array");
        assert_eq!(obj.get("minItems").unwrap().as_u64().unwrap(), 2);
        assert_eq!(obj.get("maxItems").unwrap().as_u64().unwrap(), 2);

        let items = obj.get("items").unwrap().as_object().unwrap();
        assert_eq!(items.get("type").unwrap().as_str().unwrap(), "array");
        assert_eq!(items.get("minItems").unwrap().as_u64().unwrap(), 3);
        assert_eq!(items.get("maxItems").unwrap().as_u64().unwrap(), 3);
    }
}
