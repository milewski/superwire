use super::error::ToolError;
use super::traits::Tool;
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::Schema;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
pub struct DoneArguments<O> {
    pub(crate) output: O,
}

pub struct DoneTool<O>
where
    O: Send + Sync,
{
    parameters_schema: Schema,
    phantom: PhantomData<O>,
}

impl<O> DoneTool<O>
where
    O: Send + Sync + Serialize + serde::de::DeserializeOwned + schemars::JsonSchema,
{
    pub const NAME: &'static str = "done";

    pub fn new() -> Result<Self, ToolError> {
        let parameters_schema = schemars::schema_for!(DoneArguments<O>);

        Ok(Self {
            parameters_schema,
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

impl<O> Clone for DoneTool<O>
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
impl<O> Tool for DoneTool<O>
where
    O: Send + Sync + serde::de::DeserializeOwned + Serialize + schemars::JsonSchema,
{
    type Input = DoneArguments<O>;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn description(&self) -> &'static str {
        "Call this tool when you have completed the task."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input.output).map_err(|error| {
            ToolError::new(format!("Failed to serialize output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly")
        })
    }
}
