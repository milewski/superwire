use std::fmt::Debug;
use crate::tool::ToolError;
use crate::traits::ToolDefinition;
use serde_json::Value;

/// Trait for tools that can be used by the agent
#[async_trait::async_trait]
pub trait Tool: Clone + Send + Sync {
    type Input: serde::de::DeserializeOwned + schemars::JsonSchema + Send;

    fn name(&self) -> &str;
    fn description(&self) -> &str;

    fn parameters_schema() -> schemars::Schema {
        schemars::schema_for!(Self::Input)
    }

    async fn execute(&self, input: Self::Input) -> Result<Value, ToolError>;
}

#[async_trait::async_trait]
pub trait RuntimeTool: Send + Sync + Debug {
    fn definition(&self) -> Result<ToolDefinition, ToolError>;

    async fn execute(&self, input: Value) -> Result<Value, ToolError>;
}

#[async_trait::async_trait]
impl<T> RuntimeTool for T
where
    T: Tool + Send + Sync + Debug,
{
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        Ok(ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: T::parameters_schema(),
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let input = serde_json::from_value(input).map_err(|error| {
            ToolError::new(format!(
                "Failed to deserialize tool input for '{}': {error}",
                self.name()
            ))
            .with_suggestion("Check that the arguments match the expected schema")
        })?;

        self.execute(input).await
    }
}
