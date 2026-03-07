use crate::tools::Tool;
use anyhow::Result;
use serde_json::Value;

pub struct DoneTool;

impl Tool for DoneTool {
    fn name(&self) -> &str {
        "done"
    }

    fn description(&self) -> &str {
        "Call this tool with your final output to exit the agent loop"
    }

    fn execute(&self, args: Value) -> Result<Value> {
        Ok(args)
    }
}
