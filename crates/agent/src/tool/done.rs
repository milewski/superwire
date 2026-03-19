use super::error::ToolError;
use super::traits::Tool;
use async_trait::async_trait;
use serde::Serialize;
use std::marker::PhantomData;

pub struct DoneTool<O>
where
    O: Send + Sync,
{
    phantom: PhantomData<O>,
}

impl<O> DoneTool<O>
where
    O: Send + Sync,
{
    pub fn new() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<O> Clone for DoneTool<O>
where
    O: Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<O> Tool for DoneTool<O>
where
    O: Send + Sync + serde::de::DeserializeOwned + Serialize + schemars::JsonSchema,
{
    type Input = O;

    fn name(&self) -> &'static str {
        "done"
    }

    fn description(&self) -> &'static str {
        "Call this tool when you have completed the task."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        serde_json::to_value(&input).map_err(|error| {
            ToolError::new(format!("Failed to serialize output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly".to_string())
        })
    }
}
