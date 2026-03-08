use crate::tools::error::ToolError;
use serde_json::Value;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn parameters_schema(&self) -> Value;

    async fn execute(&self, parameters: Value) -> Result<Value, ToolError>;
}

pub type ToolRef = Arc<dyn Tool>;

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Vec<ToolRef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: ToolRef) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<ToolRef> {
        self.tools.iter().find(|tool| tool.name() == name).cloned()
    }

    pub fn list(&self) -> Vec<ToolRef> {
        self.tools.clone()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
