use crate::tool::ToolError;

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
