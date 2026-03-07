use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::error::ToolError;
use crate::tools::tool::{Tool, ToolSpec};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DoneStatus {
    Success,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DonePayload {
    pub status: DoneStatus,
    pub output: Value,
}

#[derive(Debug, Default)]
pub struct DoneTool;

#[async_trait::async_trait]
impl Tool for DoneTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "done".into(),
            description: "Complete the current agent run with either a success output or failure reason.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["success", "fail"]
                    },
                    "output": {}
                },
                "required": ["status", "output"],
                "additionalProperties": false
            }),
        }
    }

    async fn invoke(&self, input: Value) -> Result<Value, ToolError> {
        let status = input
            .get("status")
            .and_then(Value::as_str)
            .ok_or(ToolError::InvalidInput {
                message: "done.status must be a string".into(),
            })?;

        match status {
            "success" | "fail" => {}
            _ => {
                return Err(ToolError::InvalidInput {
                    message: "done.status must be `success` or `fail`".into(),
                })
            }
        }

        if !input.as_object().is_some_and(|object| object.contains_key("output")) {
            return Err(ToolError::InvalidInput {
                message: "done.output is required".into(),
            });
        }

        Ok(input)
    }
}
