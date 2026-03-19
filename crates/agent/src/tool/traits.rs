use crate::tool::ToolError;
use crate::traits::ToolDefinition;

/// Trait for tools that can be used by the agent
#[async_trait::async_trait]
pub trait Tool: Clone + Send + Sync {
    type Input: serde::de::DeserializeOwned + schemars::JsonSchema + Send;

    fn name(&self) -> &str;
    fn description(&self) -> &str;

    fn parameters_schema() -> schemars::Schema {
        schemars::schema_for!(Self::Input)
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError>;
}

#[async_trait::async_trait]
pub trait RuntimeTool: Send + Sync {
    fn definition(&self) -> Result<ToolDefinition, ToolError>;

    async fn execute_json(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;
}

#[async_trait::async_trait]
impl<T> RuntimeTool for T
where
    T: Tool + Send + Sync,
{
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        Ok(ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: T::parameters_schema(),
        })
    }

    async fn execute_json(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let parsed_input = serde_json::from_value(input).map_err(|error| {
            ToolError::new(format!(
                "Failed to deserialize tool input for '{}': {error}",
                self.name()
            ))
            .with_suggestion("Check that the arguments match the expected schema".to_string())
        })?;

        self.execute(parsed_input).await
    }
}
