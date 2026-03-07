use crate::ast::{Schema, SchemaType};
use serde_json::{json, Value};
use anyhow::Result;

pub fn compile_schema(schema: &Schema) -> Result<Value> {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (field_name, field_type) in &schema.fields {
        properties.insert(field_name.clone(), compile_schema_type(field_type)?);

        // All fields are required unless they're nullable
        if !is_nullable(field_type) {
            required.push(field_name.clone());
        }
    }

    Ok(json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    }))
}

fn compile_schema_type(schema_type: &SchemaType) -> Result<Value> {
    match schema_type {
        SchemaType::String => Ok(json!({"type": "string"})),
        SchemaType::Number => Ok(json!({"type": "number"})),
        SchemaType::Boolean => Ok(json!({"type": "boolean"})),
        SchemaType::Null => Ok(json!({"type": "null"})),
        SchemaType::Array(inner) => {
            Ok(json!({
                "type": "array",
                "items": compile_schema_type(inner)?
            }))
        }
        SchemaType::Enum(values) => {
            Ok(json!({
                "type": "string",
                "enum": values
            }))
        }
        SchemaType::Union(types) => {
            let mut any_of = Vec::new();
            for t in types {
                any_of.push(compile_schema_type(t)?);
            }
            Ok(json!({
                "anyOf": any_of
            }))
        }
    }
}

fn is_nullable(schema_type: &SchemaType) -> bool {
    match schema_type {
        SchemaType::Null => true,
        SchemaType::Union(types) => types.iter().any(|t| matches!(t, SchemaType::Null)),
        _ => false,
    }
}

pub fn validate_against_schema(data: &Value, schema: &Value) -> Result<()> {
    // Use jsonschema crate for validation
    let compiled = jsonschema::validator_for(schema)
        .map_err(|e| anyhow::anyhow!("Failed to compile schema: {}", e))?;

    if let Err(error) = compiled.validate(data) {
        return Err(anyhow::anyhow!("Validation error: {}", error));
    }

    Ok(())
}
