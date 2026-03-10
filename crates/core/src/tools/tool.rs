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

/// A tool factory function that creates a new instance of a tool
pub type ToolFactoryFn = fn() -> ToolRef;

/// Wrapper for tool factory functions to enable inventory collection
pub struct ToolFactory {
    pub factory: ToolFactoryFn,
}

impl From<ToolFactoryFn> for ToolFactory {
    fn from(factory: ToolFactoryFn) -> Self {
        Self { factory }
    }
}

inventory::collect!(ToolFactory);

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Vec<ToolRef>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create a new registry with all automatically registered tools
    #[must_use]
    pub fn with_auto_registered() -> Self {
        let mut registry = Self::new();
        for factory in inventory::iter::<ToolFactory> {
            registry.register((factory.factory)());
        }
        registry
    }

    pub fn register(&mut self, tool: ToolRef) {
        self.tools.push(tool);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<ToolRef> {
        self.tools.iter().find(|tool| tool.name() == name).cloned()
    }

    #[must_use]
    pub fn list(&self) -> Vec<ToolRef> {
        self.tools.clone()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_auto_registered()
    }
}
