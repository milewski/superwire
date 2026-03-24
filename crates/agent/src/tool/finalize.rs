use super::error::ToolError;
use super::traits::Tool;
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::{schema_for, JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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
    /// { "output": { "type": "success", "answer": <final_json_value> } }
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
                { "output" : { "type": "success", "answer": <final_json_value> } }
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
