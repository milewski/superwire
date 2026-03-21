use super::error::ToolError;
use super::traits::Tool;
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::Schema;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
pub struct FinalizeArguments<O> {
    pub output: O,
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
    O: Send + Sync + Serialize + serde::de::DeserializeOwned + schemars::JsonSchema,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            parameters_schema: schemars::schema_for!(FinalizeArguments<O>),
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
    O: Send + Sync + serde::de::DeserializeOwned + Serialize + schemars::JsonSchema,
{
    type Input = FinalizeArguments<O>;

    fn name(&self) -> &'static str {
        "done"
    }

    fn description(&self) -> &'static str {
        "Call this tool when you have finalized the task."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input.output).map_err(|error| {
            ToolError::new(format!("Failed to serialize output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly")
        })
    }
}
