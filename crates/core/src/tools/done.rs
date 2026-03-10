use crate::impl_tool;
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

impl_tool!(DoneTool, DoneParameters, {
    name: "done",
    description: "Signal completion of the agent loop. Must be called with status 'success' and the final output. For status 'fail', provide an error message string in the output parameter.",
    schema: |self| {
        let base_schema = schema_for!(DoneParameters);
        let mut schema_value = serde_json::to_value(base_schema).unwrap();

        if let Some(ref output) = self.output_schema {
            if let Some(schema) = schema_value.as_object_mut() {
                if let Some(properties) = schema.get_mut("properties").and_then(|property| property.as_object_mut()) {
                    properties.insert("output".to_string(), output.clone());
                }
            }
        }

        schema_value
    },
    execute: |params| {
        Ok(serde_json::to_value(params).unwrap())
    }
});
