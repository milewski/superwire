use crate::ast::{TypeExpression, TypeField};
use crate::error::WorkflowError;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceType {
    pub is_collection: bool,
    pub value_type: TypeExpression,
}

#[must_use]
pub fn normalize_union_type(type_expression: TypeExpression) -> TypeExpression {
    match type_expression {
        TypeExpression::Union(union_members) => {
            let mut flattened_members = Vec::new();

            for union_member in union_members {
                match normalize_union_type(union_member) {
                    TypeExpression::Union(nested_members) => {
                        for nested_member in nested_members {
                            if !flattened_members.contains(&nested_member) {
                                flattened_members.push(nested_member);
                            }
                        }
                    }
                    normalized_member => {
                        if !flattened_members.contains(&normalized_member) {
                            flattened_members.push(normalized_member);
                        }
                    }
                }
            }

            if flattened_members.len() == 1 {
                flattened_members
                    .into_iter()
                    .next()
                    .expect("normalized union should retain its single member")
            } else {
                TypeExpression::Union(flattened_members)
            }
        }
        TypeExpression::Array(item_type) => TypeExpression::Array(Box::new(normalize_union_type(*item_type))),
        TypeExpression::FixedArray { item_type, length } => TypeExpression::FixedArray {
            item_type: Box::new(normalize_union_type(*item_type)),
            length,
        },
        TypeExpression::Object(fields) => TypeExpression::Object(
            fields
                .into_iter()
                .map(|field| TypeField {
                    name: field.name,
                    value_type: normalize_union_type(field.value_type),
                    description: field.description,
                })
                .collect(),
        ),
        TypeExpression::Tuple(tuple_items) => TypeExpression::Tuple(tuple_items.into_iter().map(normalize_union_type).collect()),
        other_type => other_type,
    }
}

pub fn resolve_type(type_expression: &TypeExpression, schemas: &BTreeMap<String, TypeExpression>) -> Result<TypeExpression, WorkflowError> {
    match type_expression {
        TypeExpression::Array(item_type) => Ok(TypeExpression::Array(Box::new(resolve_type(item_type, schemas)?))),
        TypeExpression::FixedArray { item_type, length } => Ok(TypeExpression::FixedArray {
            item_type: Box::new(resolve_type(item_type, schemas)?),
            length: *length,
        }),
        TypeExpression::NamedSchema(schema_name) => schemas
            .get(schema_name)
            .ok_or_else(|| WorkflowError::validation(format!("unknown schema reference 'schema.{schema_name}'")))
            .and_then(|named_schema| resolve_type(named_schema, schemas)),
        TypeExpression::Object(fields) => Ok(TypeExpression::Object(
            fields
                .iter()
                .map(|field| {
                    Ok(TypeField {
                        name: field.name.clone(),
                        value_type: resolve_type(&field.value_type, schemas)?,
                        description: field.description.clone(),
                    })
                })
                .collect::<Result<Vec<_>, WorkflowError>>()?,
        )),
        TypeExpression::Tuple(tuple_items) => Ok(TypeExpression::Tuple(
            tuple_items
                .iter()
                .map(|tuple_item| resolve_type(tuple_item, schemas))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        TypeExpression::Union(union_members) => Ok(normalize_union_type(TypeExpression::Union(
            union_members
                .iter()
                .map(|union_member| resolve_type(union_member, schemas))
                .collect::<Result<Vec<_>, _>>()?,
        ))),
        other_type => Ok(other_type.clone()),
    }
}

pub fn is_nullable_type(type_expression: &TypeExpression, schemas: &BTreeMap<String, TypeExpression>) -> Result<bool, WorkflowError> {
    let resolved_type = resolve_type(type_expression, schemas)?;

    match resolved_type {
        TypeExpression::Null => Ok(true),
        TypeExpression::Union(union_members) => Ok(union_members.contains(&TypeExpression::Null)),
        _ => Ok(false),
    }
}

pub fn remove_null_from_type(
    type_expression: &TypeExpression,
    schemas: &BTreeMap<String, TypeExpression>,
) -> Result<TypeExpression, WorkflowError> {
    let resolved_type = resolve_type(type_expression, schemas)?;

    match resolved_type {
        TypeExpression::Union(union_members) => {
            let mut non_null_members = union_members
                .into_iter()
                .filter(|union_member| *union_member != TypeExpression::Null)
                .collect::<Vec<_>>();

            if non_null_members.len() == 1 {
                Ok(non_null_members.remove(0))
            } else {
                Ok(TypeExpression::Union(non_null_members))
            }
        }
        other_type => Ok(other_type),
    }
}

pub fn property_type(
    type_expression: &TypeExpression,
    property_name: &str,
    schemas: &BTreeMap<String, TypeExpression>,
) -> Result<TypeExpression, WorkflowError> {
    let resolved_type = resolve_type(type_expression, schemas)?;

    match resolved_type {
        TypeExpression::Object(fields) => fields
            .into_iter()
            .find(|field| field.name == property_name)
            .map(|field| field.value_type)
            .ok_or_else(|| WorkflowError::validation(format!("property '{property_name}' does not exist"))),
        _ => Err(WorkflowError::validation(format!(
            "cannot access property '{property_name}' on a non-object value"
        ))),
    }
}
