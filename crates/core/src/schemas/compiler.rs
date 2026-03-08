use jsonschema::JSONSchema;
use serde_json::{Map, Value};

use crate::ast::{SchemaDefinition, SchemaType};
use crate::schemas::error::SchemaError;

pub fn compile_schema(schema: &SchemaDefinition) -> Result<Value, SchemaError> {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for field in &schema.fields {
        let field_schema = compile_type(&field.ty, field.description.as_deref())?;
        properties.insert(field.name.clone(), field_schema);
        required.push(Value::String(field.name.clone()));
    }

    let mut schema_map = Map::new();
    schema_map.insert("type".to_string(), Value::String("object".to_string()));
    schema_map.insert("properties".to_string(), Value::Object(properties));
    schema_map.insert("required".to_string(), Value::Array(required));
    schema_map.insert("additionalProperties".to_string(), Value::Bool(false));

    Ok(Value::Object(schema_map))
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

fn compile_type(schema_type: &SchemaType, description: Option<&str>) -> Result<Value, SchemaError> {
    let mut type_map = Map::new();

    match schema_type {
        SchemaType::String => {
            type_map.insert("type".to_string(), Value::String("string".to_string()));
        }
        SchemaType::Number => {
            type_map.insert("type".to_string(), Value::String("number".to_string()));
        }
        SchemaType::Boolean => {
            type_map.insert("type".to_string(), Value::String("boolean".to_string()));
        }
        SchemaType::Null => {
            type_map.insert("type".to_string(), Value::String("null".to_string()));
        }
        SchemaType::Array(inner) => {
            type_map.insert("type".to_string(), Value::String("array".to_string()));
            type_map.insert("items".to_string(), compile_type(inner, None)?);
        }
        SchemaType::Union(variants) => {
            let any_of = variants
                .iter()
                .map(|v| compile_type(v, None))
                .collect::<Result<Vec<_>, _>>()?;
            type_map.insert("anyOf".to_string(), Value::Array(any_of));
        }
        SchemaType::LiteralString(literal) => {
            type_map.insert("type".to_string(), Value::String("string".to_string()));
            type_map.insert("enum".to_string(), Value::Array(vec![Value::String(literal.clone())]));
        }
        SchemaType::Reference(name) => return Err(SchemaError::UnsupportedReference { name: name.clone() }),
    }

    if let Some(desc) = description {
        type_map.insert("description".to_string(), Value::String(desc.to_string()));
    }

    Ok(Value::Object(type_map))
}
