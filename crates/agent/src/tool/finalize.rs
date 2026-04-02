use super::error::ToolError;
use super::traits::Tool;
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::{schema_for, JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
pub struct FinalizeSuccessArguments<O> {
    /// Final successful output payload.
    pub answer: O,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FinalizeErrorArguments {
    /// Human-readable reason describing why the task could not be completed.
    pub reason: String,
}

pub struct FinalizeSuccessTool<O>
where
    O: Send + Sync,
{
    parameters_schema: Schema,
    phantom: PhantomData<O>,
}

#[derive(Debug, Clone)]
pub struct FinalizeErrorTool {
    parameters_schema: Schema,
}

impl<O> FinalizeSuccessTool<O>
where
    O: Send + Sync + Serialize + DeserializeOwned + JsonSchema,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            parameters_schema: schema_for!(FinalizeSuccessArguments<O>),
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
        let mut generated_finalize_schema = serde_json::to_value(schema_for!(FinalizeSuccessArguments<Value>))
            .map_err(|error| ToolError::new(format!("Failed to serialize finalize success schema template: {error}")))?;

        let serialized_answer_schema = serde_json::to_value(answer_schema)
            .map_err(|error| ToolError::new(format!("Failed to serialize finalize success answer schema: {error}")))?;

        let success_answer_schema_slot = generated_finalize_schema
            .as_object_mut()
            .and_then(|schema_object| schema_object.get_mut("properties"))
            .and_then(Value::as_object_mut)
            .and_then(|properties| properties.get_mut("answer"))
            .ok_or_else(|| ToolError::new("Failed to locate answer schema slot in finalize success schema template"))?;

        *success_answer_schema_slot = serialized_answer_schema;

        serde_json::from_value(generated_finalize_schema)
            .map_err(|error| ToolError::new(format!("Failed to build finalize success parameters schema: {error}")))
    }
}

impl FinalizeErrorTool {
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            parameters_schema: schema_for!(FinalizeErrorArguments),
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
}

impl<O> Clone for FinalizeSuccessTool<O>
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
impl Tool for FinalizeErrorTool {
    type Input = FinalizeErrorArguments;

    fn name(&self) -> &'static str {
        "finalize_error"
    }

    fn description(&self) -> &'static str {
        r#"
            Call this tool only when you are done and the task cannot be completed.
            Arguments MUST match:
                { "reason": "<why you could not complete>" }
        "#
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input).map_err(|error| {
            ToolError::new(format!("Failed to serialize finalize error output: {error}"))
                .with_suggestion("Ensure the error payload is serializable")
        })
    }
}

#[async_trait]
impl<O> Tool for FinalizeSuccessTool<O>
where
    O: Send + Sync + DeserializeOwned + Serialize + JsonSchema,
{
    type Input = FinalizeSuccessArguments<O>;

    fn name(&self) -> &'static str {
        "finalize_success"
    }

    fn description(&self) -> &'static str {
        r#"
            Call this tool only when you are done and have a final successful result.
            Arguments MUST match:
                { "answer": <final_json_object> }
        "#
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input).map_err(|error| {
            ToolError::new(format!("Failed to serialize finalize success output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FinalizeSuccessTool;
    use crate::validate_json_against_schema_with_context;
    use schemars::Schema;
    use serde_json::{json, Value};

    #[test]
    fn injects_runtime_answer_schema_into_generated_finalize_success_schema() {
        let integer_answer_schema: Schema =
            serde_json::from_value(json!({ "type": "integer" })).expect("integer answer schema should deserialize");

        let finalize_schema = FinalizeSuccessTool::<Value>::parameters_schema_for_answer_schema(&integer_answer_schema)
            .expect("finalize success schema should be generated");

        let valid_finalize_arguments = json!({
            "answer": 42
        });

        let invalid_finalize_arguments = json!({
            "answer": {
                "random_number": 42
            }
        });

        assert!(validate_json_against_schema_with_context(&valid_finalize_arguments, &finalize_schema, "Finalize validation",).is_ok());
        assert!(validate_json_against_schema_with_context(&invalid_finalize_arguments, &finalize_schema, "Finalize validation",).is_err());
    }
}
