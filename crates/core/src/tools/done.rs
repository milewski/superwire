use crate::tools::error::ToolError;
use crate::tools::tool::Tool;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DoneStatus {
    Success,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DoneParameters {
    pub status: DoneStatus,
    pub output: Value,
}

pub struct DoneTool;

impl DoneTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DoneTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for DoneTool {
    fn name(&self) -> &str {
        "done"
    }

    fn description(&self) -> &str {
        "Signal completion of the agent loop. Must be called with status 'success' and the final output as a JSON object (not a string). The 'output' parameter must be the actual JSON object, not a string containing JSON."
    }

    fn parameters_schema(&self) -> Value {
        let schema = schema_for!(DoneParameters);
        serde_json::to_value(schema).unwrap()
    }

    async fn execute(&self, parameters: Value) -> Result<Value, ToolError> {
        let params: DoneParameters =
            serde_json::from_value(parameters).map_err(|error| ToolError::InvalidParameters {
                tool_name: "done".to_string(),
                message: format!("Failed to parse done parameters: {}", error),
                suggestion: Some("Ensure you provide 'status' (success or fail) and 'output' fields".to_string()),
            })?;

        Ok(serde_json::to_value(params).unwrap())
    }
}
