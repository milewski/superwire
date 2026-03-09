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

pub struct DoneTool {
    output_schema: Option<Value>,
}

impl DoneTool {
    #[must_use]
    pub const fn new(output_schema: Option<Value>) -> Self {
        Self { output_schema }
    }
}

impl Default for DoneTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait::async_trait]
impl Tool for DoneTool {
    fn name(&self) -> &'static str {
        "done"
    }

    fn description(&self) -> &'static str {
        "Signal completion of the agent loop. Must be called with status 'success' and the final output. For status 'fail', provide an error message string in the output parameter."
    }

    fn parameters_schema(&self) -> Value {
        let base_schema = schema_for!(DoneParameters);
        let mut schema_value = serde_json::to_value(base_schema).unwrap();

        if let Some(ref output_schema) = self.output_schema {
            if let Some(schema_obj) = schema_value.as_object_mut() {
                if let Some(properties) = schema_obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
                    properties.insert("output".to_string(), output_schema.clone());
                }
            }
        }

        schema_value
    }

    async fn execute(&self, parameters: Value) -> Result<Value, ToolError> {
        let params: DoneParameters =
            serde_json::from_value(parameters).map_err(|error| ToolError::InvalidParameters {
                tool_name: "done".to_string(),
                message: format!("Failed to parse done parameters: {error}"),
                suggestion: Some("Ensure you provide 'status' (success or fail) and 'output' fields".to_string()),
            })?;

        Ok(serde_json::to_value(params).unwrap())
    }
}
