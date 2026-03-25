use super::error::ToolError;
use super::traits::Tool;
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::{schema_for, JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FinalizeOutput<O> {
    /// Successful completion payload.
    /// The final structured output must be nested under this `answer` field.
    Success { answer: O },

    /// Failed completion payload with a concrete reason.
    Failure { reason: String },
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
pub struct FinalizeArguments<O> {
    /// Required wrapper object (top-level).
    ///
    /// Valid success shape:
    ///
    /// ```json
    /// { "output": { "type": "success", "answer": { ...final object... } } }
    /// ```
    ///
    /// Valid failure shape:
    ///
    /// ```json
    /// { "output": { "type": "failure", "reason": "..." } }
    /// ```
    pub output: FinalizeOutput<O>,
}

pub struct FinalizeTool<O>
where
    O: Send + Sync,
{
    parameters_schema: Schema,
    phantom: PhantomData<O>,
}

impl<O> FinalizeTool<O>
where
    O: Send + Sync + Serialize + DeserializeOwned + JsonSchema,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            parameters_schema: schema_for!(FinalizeArguments<O>),
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn as_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: self.parameters_schema.clone(),
        }
    }

    #[must_use]
    pub fn parameters_schema(&self) -> &Schema {
        &self.parameters_schema
    }

    pub fn parameters_schema_for_answer_schema(answer_schema: &Schema) -> Result<Schema, ToolError> {
        let mut generated_finalize_schema = serde_json::to_value(schema_for!(FinalizeArguments<Value>))
            .map_err(|error| ToolError::new(format!("Failed to serialize finalize schema template: {error}")))?;

        let serialized_answer_schema = serde_json::to_value(answer_schema)
            .map_err(|error| ToolError::new(format!("Failed to serialize finalize answer schema: {error}")))?;

        let success_answer_schema_slot = find_success_answer_schema_slot(&mut generated_finalize_schema)
            .ok_or_else(|| ToolError::new("Failed to locate success answer schema slot in finalize schema template"))?;

        *success_answer_schema_slot = serialized_answer_schema;

        serde_json::from_value(generated_finalize_schema)
            .map_err(|error| ToolError::new(format!("Failed to build finalize parameters schema: {error}")))
    }
}

fn find_success_answer_schema_slot(schema_template: &mut Value) -> Option<&mut Value> {
    match schema_template {
        Value::Object(schema_object) => {
            let is_success_branch = schema_object
                .get("properties")
                .and_then(Value::as_object)
                .and_then(|properties| {
                    properties
                        .get("type")
                        .and_then(Value::as_object)
                        .and_then(|type_schema| type_schema.get("const"))
                        .and_then(Value::as_str)
                        .map(|constant_value| constant_value == "success" && properties.contains_key("answer"))
                })
                .unwrap_or(false);

            if is_success_branch {
                return schema_object
                    .get_mut("properties")
                    .and_then(Value::as_object_mut)
                    .and_then(|properties| properties.get_mut("answer"));
            }

            for nested_schema in schema_object.values_mut() {
                if let Some(answer_slot) = find_success_answer_schema_slot(nested_schema) {
                    return Some(answer_slot);
                }
            }

            None
        }
        Value::Array(schema_array) => {
            for nested_schema in schema_array {
                if let Some(answer_slot) = find_success_answer_schema_slot(nested_schema) {
                    return Some(answer_slot);
                }
            }

            None
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

impl<O> Clone for FinalizeTool<O>
where
    O: Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            parameters_schema: self.parameters_schema.clone(),
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<O> Tool for FinalizeTool<O>
where
    O: Send + Sync + DeserializeOwned + Serialize + JsonSchema,
{
    type Input = FinalizeArguments<O>;

    fn name(&self) -> &'static str {
        "finalize"
    }

    fn description(&self) -> &'static str {
        r#"
            Call this tool only when you are done.
            Arguments MUST be exactly one of:
                { "output" : { "type": "success", "answer": <final_json_object> } }
                { "output" : { "type": "failure", "reason": "<why you could not complete>" } }

            Required success keys: output.type and output.answer
            Required failure keys: output.type and output.reason

            Important:
            - `type` is nested inside `output`, never at the top level.
            - There is only one `output` object.
            - DO NOT send output.output.answer.
            - Correct path is output.answer.
        "#
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input.output).map_err(|error| {
            ToolError::new(format!("Failed to serialize output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FinalizeTool;
    use crate::validate_json_against_schema_with_context;
    use schemars::Schema;
    use serde_json::{json, Value};

    #[test]
    fn injects_runtime_answer_schema_into_generated_finalize_schema() {
        let integer_answer_schema: Schema =
            serde_json::from_value(json!({ "type": "integer" })).expect("integer answer schema should deserialize");

        let finalize_schema = FinalizeTool::<Value>::parameters_schema_for_answer_schema(&integer_answer_schema)
            .expect("finalize schema should be generated");

        let valid_finalize_arguments = json!({
            "output": {
                "type": "success",
                "answer": 42
            }
        });

        let invalid_finalize_arguments = json!({
            "output": {
                "type": "success",
                "answer": {
                    "random_number": 42
                }
            }
        });

        assert!(validate_json_against_schema_with_context(&valid_finalize_arguments, &finalize_schema, "Finalize validation",).is_ok());
        assert!(validate_json_against_schema_with_context(&invalid_finalize_arguments, &finalize_schema, "Finalize validation",).is_err());
    }
}
