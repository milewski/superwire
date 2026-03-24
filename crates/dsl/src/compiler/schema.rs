use crate::ast::{PrimitiveType, TypeExpression, TypeField};
use crate::compiler::types::{normalize_union_type, resolve_type};
use crate::error::WorkflowError;
use schemars::Schema;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub fn build_object_schema(fields: &[TypeField], schemas: &BTreeMap<String, TypeExpression>) -> Result<Schema, WorkflowError> {
    build_schema_from_value(json!({
        "type": "object",
        "properties": build_object_properties(fields, schemas)?,
        "required": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
        "additionalProperties": false,
    }))
}

pub fn build_type_schema(type_expression: &TypeExpression, schemas: &BTreeMap<String, TypeExpression>) -> Result<Schema, WorkflowError> {
    let resolved_type = normalize_union_type(resolve_type(type_expression, schemas)?);
    build_type_schema_value(&resolved_type, schemas).and_then(build_schema_from_value)
}

fn build_type_schema_value(type_expression: &TypeExpression, schemas: &BTreeMap<String, TypeExpression>) -> Result<Value, WorkflowError> {
    match type_expression {
        TypeExpression::Array(item_type) => Ok(json!({
            "type": "array",
            "items": build_type_schema_value(item_type, schemas)?,
        })),
        TypeExpression::FixedArray { item_type, length } => Ok(json!({
            "type": "array",
            "items": build_type_schema_value(item_type, schemas)?,
            "minItems": length,
            "maxItems": length,
        })),
        TypeExpression::NamedSchema(_) => unreachable!("named schemas should be resolved before schema generation"),
        TypeExpression::Null => Ok(json!({ "type": "null" })),
        TypeExpression::Object(fields) => Ok(json!({
            "type": "object",
            "properties": build_object_properties(fields, schemas)?,
            "required": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
            "additionalProperties": false,
        })),
        TypeExpression::Primitive(primitive_type) => Ok(match primitive_type {
            PrimitiveType::Boolean => json!({ "type": "boolean" }),
            PrimitiveType::Float => json!({ "type": "number" }),
            PrimitiveType::Number => json!({ "type": "integer" }),
            PrimitiveType::String => json!({ "type": "string" }),
        }),
        TypeExpression::StringLiteral(string_literal) => Ok(json!({
            "type": "string",
            "const": string_literal,
        })),
        TypeExpression::Tuple(tuple_items) => Ok(json!({
            "type": "array",
            "prefixItems": tuple_items
                .iter()
                .map(|tuple_item| build_type_schema_value(tuple_item, schemas))
                .collect::<Result<Vec<_>, _>>()?,
            "minItems": tuple_items.len(),
            "maxItems": tuple_items.len(),
        })),
        TypeExpression::Union(union_members) => {
            let string_literals = union_members
                .iter()
                .filter_map(|union_member| match union_member {
                    TypeExpression::StringLiteral(string_literal) => Some(string_literal.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            if !string_literals.is_empty() && string_literals.len() == union_members.len() {
                return Ok(json!({
                    "type": "string",
                    "enum": string_literals,
                }));
            }

            Ok(json!({
                "oneOf": union_members
                    .iter()
                    .map(|union_member| build_type_schema_value(union_member, schemas))
                    .collect::<Result<Vec<_>, _>>()?,
            }))
        }
    }
}

fn build_object_properties(fields: &[TypeField], schemas: &BTreeMap<String, TypeExpression>) -> Result<Map<String, Value>, WorkflowError> {
    let mut properties = Map::new();

    for field in fields {
        let resolved_field_type = resolve_type(&field.value_type, schemas)?;
        let mut field_schema = build_type_schema_value(&resolved_field_type, schemas)?;

        if let Some(description) = &field.description {
            field_schema
                .as_object_mut()
                .expect("field schemas should always serialize as objects")
                .insert("description".to_string(), Value::String(description.clone()));
        }

        properties.insert(field.name.clone(), field_schema);
    }

    Ok(properties)
}

fn build_schema_from_value(schema_value: Value) -> Result<Schema, WorkflowError> {
    serde_json::from_value(schema_value).map_err(|error| WorkflowError::schema(format!("failed to build JSON schema: {error}")))
}
