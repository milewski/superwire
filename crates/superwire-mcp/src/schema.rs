use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::Serialize;
use serde_json::Value;
use superwire_semantic::support::types::{workflow_type_from_json_schema, WorkflowType};
use superwire_semantic::WorkflowSemanticError;
use superwire_types::ast::{SourceSpan, TypeExpression, TypedField};

impl super::McpToolLock {
    pub fn input_schema_value(&self) -> Result<Value, WorkflowSemanticError> {
        serialize_schema(&self.input_schema, "MCP tool input schema")
    }

    pub fn output_schema_value(&self) -> Result<Option<Value>, WorkflowSemanticError> {
        self.output_schema
            .as_ref()
            .map(|output_schema| serialize_schema(output_schema, "MCP tool output schema"))
            .transpose()
    }

    pub fn input_fields_except(&self, excluded_field_names: &[&str]) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        self.input_schema.typed_fields_except(excluded_field_names)
    }

    pub fn output_fields(&self) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        self.output_schema
            .as_ref()
            .map(TypedJsonSchema::typed_fields)
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

trait TypedJsonSchema {
    fn typed_fields(&self) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        self.typed_fields_except(&[])
    }

    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Result<Vec<TypedField>, WorkflowSemanticError>;

    #[cfg(test)]
    fn type_expression(&self) -> Result<TypeExpression, WorkflowSemanticError>;
}

impl TypedJsonSchema for ToolInputSchema {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        serialize_schema(self, "MCP tool input schema")?.typed_fields_except(excluded_field_names)
    }

    #[cfg(test)]
    fn type_expression(&self) -> Result<TypeExpression, WorkflowSemanticError> {
        serialize_schema(self, "MCP tool input schema")?.type_expression()
    }
}

impl TypedJsonSchema for ToolOutputSchema {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        serialize_schema(self, "MCP tool output schema")?.typed_fields_except(excluded_field_names)
    }

    #[cfg(test)]
    fn type_expression(&self) -> Result<TypeExpression, WorkflowSemanticError> {
        serialize_schema(self, "MCP tool output schema")?.type_expression()
    }
}

impl TypedJsonSchema for Value {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Result<Vec<TypedField>, WorkflowSemanticError> {
        let workflow_type = workflow_type_from_json_schema(self)?;
        let WorkflowType::Object(fields) = workflow_type else {
            if matches!(workflow_type, WorkflowType::AnyObject | WorkflowType::Any) {
                return Ok(Vec::new());
            }

            return Err(WorkflowSemanticError::Other {
                message: format!("MCP tool schema root must be an object, found {workflow_type}"),
            });
        };
        let properties = self.get("properties").and_then(Value::as_object);
        let typed_fields = fields
            .into_iter()
            .filter(|(field_name, _)| !excluded_field_names.contains(&field_name.as_str()))
            .map(|(field_name, field_type)| {
                let field_schema = properties.and_then(|property_schemas| property_schemas.get(&field_name));
                let description = field_schema
                    .and_then(|schema| schema.get("description"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                TypedField::from_type_with_description(
                    field_name,
                    field_type.to_mcp_type_expression(field_schema),
                    description,
                    SourceSpan::generated(),
                )
            })
            .collect();

        Ok(typed_fields)
    }

    #[cfg(test)]
    fn type_expression(&self) -> Result<TypeExpression, WorkflowSemanticError> {
        workflow_type_from_json_schema(self).map(|workflow_type| workflow_type.to_mcp_type_expression(Some(self)))
    }
}

trait McpWorkflowTypeExpression {
    fn to_mcp_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression;
    fn to_mcp_object_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression;

    fn to_mcp_variant_type_expression(&self) -> TypeExpression;

    fn to_mcp_union_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression;
}

impl McpWorkflowTypeExpression for WorkflowType {
    fn to_mcp_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression {
        match self {
            Self::Any => TypeExpression::AnyObject,
            Self::String => TypeExpression::String,
            Self::Integer => TypeExpression::Number,
            Self::Float => TypeExpression::Float,
            Self::Boolean => TypeExpression::Boolean,
            Self::Null => TypeExpression::Null,
            Self::AnyObject => TypeExpression::AnyObject,
            Self::StringEnum(enum_values) => {
                let mut enum_type_expressions = enum_values.iter().cloned().map(TypeExpression::StringEnum).collect::<Vec<_>>();

                if enum_type_expressions.len() == 1 {
                    enum_type_expressions.pop().expect("single string enum type should exist")
                } else {
                    TypeExpression::Union(enum_type_expressions)
                }
            }
            Self::Array { item_type, fixed_length } => TypeExpression::Array {
                item_type: Box::new(item_type.to_mcp_type_expression(schema_value.and_then(|schema| schema.get("items")))),
                fixed_length: *fixed_length,
            },
            Self::Tuple(item_types) => TypeExpression::Tuple(
                item_types
                    .iter()
                    .enumerate()
                    .map(|(item_index, item_type)| {
                        let item_schema = schema_value
                            .and_then(|schema| schema.get("prefixItems").or_else(|| schema.get("items")))
                            .and_then(Value::as_array)
                            .and_then(|item_schemas| item_schemas.get(item_index));

                        item_type.to_mcp_type_expression(item_schema)
                    })
                    .collect(),
            ),
            Self::Object(_) => self.to_mcp_object_type_expression(schema_value),
            Self::Variant { .. } => self.to_mcp_variant_type_expression(),
            Self::Union(_) => self.to_mcp_union_type_expression(schema_value),
        }
    }

    fn to_mcp_object_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression {
        let Self::Object(fields) = self else {
            unreachable!("object type conversion requires object fields");
        };
        let property_schemas = schema_value.and_then(|schema| schema.get("properties")).and_then(Value::as_object);
        let typed_fields = fields
            .iter()
            .map(|(field_name, field_type)| {
                let field_schema = property_schemas.and_then(|properties| properties.get(field_name));
                let description = field_schema
                    .and_then(|schema| schema.get("description"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                TypedField::from_type_with_description(
                    field_name.clone(),
                    field_type.to_mcp_type_expression(field_schema),
                    description,
                    SourceSpan::generated(),
                )
            })
            .collect();

        TypeExpression::Object(typed_fields)
    }

    fn to_mcp_variant_type_expression(&self) -> TypeExpression {
        let Self::Variant { discriminator, cases } = self else {
            unreachable!("variant type conversion requires variant cases");
        };

        TypeExpression::Variant {
            discriminator: discriminator.clone(),
            cases: cases
                .iter()
                .map(|(case_name, fields)| superwire_types::ast::VariantCase {
                    name: case_name.clone(),
                    fields: fields
                        .iter()
                        .filter(|(field_name, _)| *field_name != discriminator)
                        .map(|(field_name, field_type)| {
                            TypedField::from_type(field_name.clone(), field_type.to_mcp_type_expression(None), SourceSpan::generated())
                        })
                        .collect(),
                    span: SourceSpan::generated(),
                })
                .collect(),
        }
    }

    fn to_mcp_union_type_expression(&self, schema_value: Option<&Value>) -> TypeExpression {
        let Self::Union(type_expressions) = self else {
            unreachable!("union type conversion requires union members");
        };
        let mut mcp_type_expressions = Vec::new();

        for type_expression in type_expressions
            .iter()
            .filter(|type_expression| !matches!(type_expression, Self::Null))
        {
            match type_expression.to_mcp_type_expression(schema_value) {
                TypeExpression::Union(nested_type_expressions) => mcp_type_expressions.extend(nested_type_expressions),
                mcp_type_expression => mcp_type_expressions.push(mcp_type_expression),
            }
        }

        if type_expressions.iter().any(|type_expression| matches!(type_expression, Self::Null)) {
            mcp_type_expressions.push(TypeExpression::Null);
        }

        TypeExpression::Union(mcp_type_expressions)
    }
}

fn serialize_schema(schema: &impl Serialize, context: &str) -> Result<Value, WorkflowSemanticError> {
    serde_json::to_value(schema).map_err(|source| WorkflowSemanticError::SerializationFailed {
        context: context.to_string(),
        source,
    })
}

pub(super) fn to_json_value(schema: &impl Serialize) -> Result<Value, serde_json::Error> {
    serde_json::to_value(schema)
}

#[cfg(test)]
mod tests {
    use super::TypedJsonSchema;
    use serde_json::json;
    use superwire_types::ast::TypeExpression;

    #[test]
    fn type_expression_supports_nullable_array_type_keyword() {
        let schema = json!({
            "type": ["array", "null"],
            "items": {
                "type": "string"
            }
        });

        let type_expression = schema.type_expression().expect("nullable array schema should convert");

        assert_eq!(
            type_expression,
            TypeExpression::Union(vec![
                TypeExpression::Array {
                    item_type: Box::new(TypeExpression::String),
                    fixed_length: None,
                },
                TypeExpression::Null,
            ])
        );
    }

    #[test]
    fn type_expression_supports_nullable_integer_type_keyword() {
        let schema = json!({
            "type": ["integer", "null"],
        });

        let type_expression = schema.type_expression().expect("nullable integer schema should convert");

        assert_eq!(
            type_expression,
            TypeExpression::Union(vec![TypeExpression::Number, TypeExpression::Null])
        );
    }

    #[test]
    fn type_expression_supports_nullable_string_enum_type_keyword() {
        let schema = json!({
            "type": ["string", "null"],
            "enum": ["picture", "video_recording", null],
        });

        let type_expression = schema.type_expression().expect("nullable enum schema should convert");

        assert_eq!(
            type_expression,
            TypeExpression::Union(vec![
                TypeExpression::StringEnum("picture".to_string()),
                TypeExpression::StringEnum("video_recording".to_string()),
                TypeExpression::Null,
            ])
        );
    }

    #[test]
    fn typed_fields_include_nullable_properties_that_are_not_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "integer"
                },
                "task_group_id": {
                    "type": ["integer", "null"]
                },
                "comment": {
                    "type": "string"
                }
            },
            "required": ["project_id"]
        });

        let typed_fields = schema.typed_fields().expect("object schema should convert");
        let mut field_names = typed_fields.iter().map(|typed_field| typed_field.name.as_str()).collect::<Vec<_>>();
        field_names.sort_unstable();

        assert_eq!(field_names, vec!["comment", "project_id", "task_group_id"]);
        let task_group_field = typed_fields
            .iter()
            .find(|typed_field| typed_field.name == "task_group_id")
            .expect("nullable task group field should be retained");

        assert_eq!(
            task_group_field.field_type,
            TypeExpression::Union(vec![TypeExpression::Number, TypeExpression::Null])
        );
        let comment_field = typed_fields
            .iter()
            .find(|typed_field| typed_field.name == "comment")
            .expect("optional non-null comment field should be retained");

        assert_eq!(
            comment_field.field_type,
            TypeExpression::Union(vec![TypeExpression::String, TypeExpression::Null])
        );
    }

    #[test]
    fn typed_fields_preserve_integer_and_number_distinction() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer"
                },
                "ratio": {
                    "type": "number"
                }
            },
            "required": ["count", "ratio"]
        });

        let typed_fields = schema.typed_fields().expect("numeric object schema should convert");
        let count_field = typed_fields
            .iter()
            .find(|typed_field| typed_field.name == "count")
            .expect("integer field should exist");
        let ratio_field = typed_fields
            .iter()
            .find(|typed_field| typed_field.name == "ratio")
            .expect("number field should exist");

        assert_eq!(count_field.field_type, TypeExpression::Number);
        assert_eq!(ratio_field.field_type, TypeExpression::Float);
    }
}
