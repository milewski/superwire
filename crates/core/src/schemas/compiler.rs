use jsonschema::JSONSchema;
use serde_json::{json, Map, Value};

use crate::ast::{SchemaDefinition, SchemaType};
use crate::schemas::error::SchemaError;

pub fn compile_schema(schema: &SchemaDefinition) -> Result<Value, SchemaError> {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for field in &schema.fields {
        let mut field_schema = compile_type(&field.ty)?;

        if let Some(description) = &field.description {
            if let Value::Object(ref mut obj) = field_schema {
                obj.insert("description".into(), Value::String(description.clone()));
            }
        }

        properties.insert(field.name.clone(), field_schema);
        required.push(Value::String(field.name.clone()));
    }

    Ok(Value::Object(Map::from_iter([
        ("type".into(), Value::String("object".into())),
        ("properties".into(), Value::Object(properties)),
        ("required".into(), Value::Array(required)),
        ("additionalProperties".into(), Value::Bool(false)),
    ])))
}

pub fn validate_value(schema: &SchemaDefinition, value: &Value) -> Result<(), SchemaError> {
    let json_schema = compile_schema(schema)?;
    let compiled = JSONSchema::compile(&json_schema).map_err(|source| SchemaError::Compile {
        message: source.to_string(),
    })?;

    if let Err(errors) = compiled.validate(value) {
        let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>();
        return Err(SchemaError::Validation {
            messages: messages.join("; "),
        });
    }

    Ok(())
}

fn compile_type(schema_type: &SchemaType) -> Result<Value, SchemaError> {
    let value = match schema_type {
        SchemaType::String => json!({ "type": "string" }),
        SchemaType::Number => json!({ "type": "number" }),
        SchemaType::Boolean => json!({ "type": "boolean" }),
        SchemaType::Null => json!({ "type": "null" }),
        SchemaType::Array(inner) => json!({
            "type": "array",
            "items": compile_type(inner)?,
        }),
        SchemaType::Union(variants) => Value::Object(Map::from_iter([(
            "anyOf".into(),
            Value::Array(
                variants
                    .iter()
                    .map(compile_type)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )])),
        SchemaType::LiteralString(literal) => json!({
            "type": "string",
            "enum": [literal],
        }),
        SchemaType::Reference(name) => {
            return Err(SchemaError::UnsupportedReference {
                name: name.clone(),
            })
        }
    };

    Ok(value)
}
