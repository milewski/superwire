use crate::dsl::{SourcePosition, SourceSpan, TypeExpression, TypedField};
use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::Serialize;
use serde_json::Value;

impl super::McpToolLock {
    #[must_use]
    pub(super) fn input_fields_except(&self, excluded_field_names: &[&str]) -> Vec<TypedField> {
        self.input_schema.typed_fields_except(excluded_field_names)
    }

    #[must_use]
    pub(super) fn output_fields(&self) -> Vec<TypedField> {
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
            .map(|required| required.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        let include_all_fields = required_fields.is_empty();
        let mut typed_fields = Vec::new();

        for (field_name, field_schema) in properties {
            if excluded_field_names.contains(&field_name.as_str()) {
                continue;
            }

            if !include_all_fields && !required_fields.contains(&field_name.as_str()) {
                continue;
            }

            typed_fields.push(TypedField {
                name: field_name.clone(),
                field_type: field_schema.type_expression(),
                description: field_schema.get("description").and_then(Value::as_str).map(str::to_string),
                span: SourceSpan::generated(),
            });
        }

        typed_fields
    }

    fn type_expression(&self) -> TypeExpression {
        if let Some(enum_values) = self.get("enum").and_then(Value::as_array) {
            let mut string_enum_values = enum_values
                .iter()
                .filter_map(Value::as_str)
                .map(|enum_value| TypeExpression::StringEnum(enum_value.to_string()))
                .collect::<Vec<_>>();

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

        match self.get("type").and_then(Value::as_str) {
            Some("string") => TypeExpression::String,
            Some("integer" | "number") => TypeExpression::Number,
            Some("boolean") => TypeExpression::Boolean,
            Some("null") => TypeExpression::Null,
            Some("array") => TypeExpression::Array {
                item_type: Box::new(self.get("items").map_or(TypeExpression::String, TypedJsonSchema::type_expression)),
                fixed_length: None,
            },
            Some("object") => TypeExpression::Object(self.typed_fields()),
            _ => TypeExpression::String,
        }
    }
}

pub(super) fn to_json_value(schema: &impl Serialize) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Null)
}

impl SourceSpan {
    fn generated() -> Self {
        Self {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
