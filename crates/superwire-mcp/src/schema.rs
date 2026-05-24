use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::Serialize;
use serde_json::Value;
use superwire_types::ast::{SourceSpan, TypeExpression, TypedField};

impl super::McpToolLock {
    #[must_use]
    pub fn input_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField> {
        self.input_schema.typed_fields_except(excluded_field_names)
    }

    #[must_use]
    pub fn output_fields(&self) -> Vec<TypedField> {
        self.output_schema.as_ref().map(TypedJsonSchema::typed_fields).unwrap_or_default()
    }
}

pub(super) trait TypedJsonSchema {
    fn typed_fields(&self) -> Vec<TypedField> {
        self.typed_fields_except(&[])
    }

    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField>;

    fn type_expression(&self) -> TypeExpression;
}

impl TypedJsonSchema for ToolInputSchema {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField> {
        let schema_value = serde_json::to_value(self).unwrap_or(Value::Null);

        schema_value.typed_fields_except(excluded_field_names)
    }

    fn type_expression(&self) -> TypeExpression {
        let schema_value = serde_json::to_value(self).unwrap_or(Value::Null);

        schema_value.type_expression()
    }
}

impl TypedJsonSchema for ToolOutputSchema {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField> {
        let schema_value = serde_json::to_value(self).unwrap_or(Value::Null);

        schema_value.typed_fields_except(excluded_field_names)
    }

    fn type_expression(&self) -> TypeExpression {
        let schema_value = serde_json::to_value(self).unwrap_or(Value::Null);

        schema_value.type_expression()
    }
}

impl TypedJsonSchema for Value {
    fn typed_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField> {
        let Some(properties) = self.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        let required_fields = self
            .get("required")
            .and_then(Value::as_array)
            .map(|required| required.iter().filter_map(Value::as_str).collect::<Vec<_>>());
        let include_all_fields = required_fields.is_none();
        let required_fields = required_fields.unwrap_or_default();
        let mut typed_fields = Vec::new();

        for (field_name, field_schema) in properties {
            if excluded_field_names.contains(&field_name.as_str()) {
                continue;
            }

            let field_type = field_schema.type_expression();

            if !include_all_fields && !required_fields.contains(&field_name.as_str()) && !field_type.can_be_null() {
                continue;
            }

            typed_fields.push(TypedField {
                name: field_name.clone(),
                field_type,
                description: field_schema.get("description").and_then(Value::as_str).map(str::to_string),
                span: SourceSpan::generated(),
            });
        }

        typed_fields
    }

    fn type_expression(&self) -> TypeExpression {
        let type_expression_for_keyword = |type_keyword: &str| -> Option<TypeExpression> {
            match type_keyword {
                "string" => Some(TypeExpression::String),
                "integer" | "number" => Some(TypeExpression::Number),
                "boolean" => Some(TypeExpression::Boolean),
                "null" => Some(TypeExpression::Null),
                "array" => Some(TypeExpression::Array {
                    item_type: Box::new(self.get("items").map_or(TypeExpression::String, TypedJsonSchema::type_expression)),
                    fixed_length: None,
                }),
                "object" => Some(TypeExpression::Object(self.typed_fields())),
                _ => None,
            }
        };

        if let Some(enum_values) = self.get("enum").and_then(Value::as_array) {
            let mut string_enum_values = enum_values
                .iter()
                .filter_map(Value::as_str)
                .map(|enum_value| TypeExpression::StringEnum(enum_value.to_string()))
                .collect::<Vec<_>>();

            if self.allows_null_type_keyword() {
                string_enum_values.push(TypeExpression::Null);
            }

            if string_enum_values.len() == 1 {
                return string_enum_values.remove(0);
            }

            if !string_enum_values.is_empty() {
                return TypeExpression::Union(string_enum_values);
            }
        }

        if let Some(one_of) = self.get("oneOf").and_then(Value::as_array) {
            return TypeExpression::Union(one_of.iter().map(TypedJsonSchema::type_expression).collect());
        }

        if let Some(any_of) = self.get("anyOf").and_then(Value::as_array) {
            return TypeExpression::Union(any_of.iter().map(TypedJsonSchema::type_expression).collect());
        }

        if let Some(type_keywords) = self.get("type").and_then(Value::as_array) {
            let mut type_expressions = type_keywords
                .iter()
                .filter_map(Value::as_str)
                .filter_map(type_expression_for_keyword)
                .collect::<Vec<_>>();

            if type_expressions.len() == 1 {
                return type_expressions.remove(0);
            }

            if !type_expressions.is_empty() {
                return TypeExpression::Union(type_expressions);
            }
        }

        match self.get("type").and_then(Value::as_str) {
            Some(type_keyword) => type_expression_for_keyword(type_keyword).unwrap_or(TypeExpression::String),
            _ => TypeExpression::String,
        }
    }
}

trait JsonSchemaValueExt {
    fn allows_null_type_keyword(&self) -> bool;
}

impl JsonSchemaValueExt for Value {
    fn allows_null_type_keyword(&self) -> bool {
        match self.get("type") {
            Some(Value::String(type_keyword)) => type_keyword == "null",
            Some(Value::Array(type_keywords)) => type_keywords.iter().any(|type_keyword| type_keyword.as_str() == Some("null")),
            _ => false,
        }
    }
}

pub(super) fn to_json_value(schema: &impl Serialize) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Null)
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

        let type_expression = schema.type_expression();

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

        let type_expression = schema.type_expression();

        assert_eq!(
            type_expression,
            TypeExpression::Union(vec![TypeExpression::Number, TypeExpression::Null])
        );
    }

    #[test]
    fn type_expression_supports_nullable_string_enum_type_keyword() {
        let schema = json!({
            "type": ["string", "null"],
            "enum": ["picture", "video_recording"],
        });

        let type_expression = schema.type_expression();

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

        let typed_fields = schema.typed_fields();
        let mut field_names = typed_fields.iter().map(|typed_field| typed_field.name.as_str()).collect::<Vec<_>>();
        field_names.sort_unstable();

        assert_eq!(field_names, vec!["project_id", "task_group_id"]);
        let task_group_field = typed_fields
            .iter()
            .find(|typed_field| typed_field.name == "task_group_id")
            .expect("nullable task group field should be retained");

        assert_eq!(
            task_group_field.field_type,
            TypeExpression::Union(vec![TypeExpression::Number, TypeExpression::Null])
        );
    }
}
