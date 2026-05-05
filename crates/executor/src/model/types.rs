use crate::event::ExecutorEvent;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::provider::OpenAIProviderConfig;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub agent_name: String,
    pub provider_config: OpenAIProviderConfig,
    pub model_name: String,
    pub prompt: String,
    pub output_schema: Value,
    pub tools: Vec<ModelToolDefinition>,
    pub event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    pub mcp_pool: McpClientPool,
    pub tool_call_tracker: ToolCallTracker,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub source: ModelToolSource,
    pub input_schema: Value,
    pub output_schema: Value,
    pub bindings: Value,
    pub max_calls: Option<u64>,
    pub max_calls_scope: ToolCallLimitScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallLimitScope {
    Workflow,
    Agent { agent_name: String },
}

impl ToolCallLimitScope {
    #[must_use]
    pub fn key(&self, tool_name: &str) -> String {
        match self {
            Self::Workflow => tool_name.to_string(),
            Self::Agent { agent_name } => format!("{agent_name}::{tool_name}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallTracker {
    counts_by_tool: Arc<Mutex<HashMap<String, u64>>>,
}

impl ToolCallTracker {
    pub fn register_call(&self, tool_name: &str, max_calls: Option<u64>, scope: &ToolCallLimitScope) -> Result<(), String> {
        let Some(max_calls) = max_calls else {
            return Ok(());
        };

        let mut counts_by_tool = self
            .counts_by_tool
            .lock()
            .map_err(|error| format!("failed to acquire tool call tracker lock: {error}"))?;
        let scope_key = scope.key(tool_name);
        let current_count = counts_by_tool.entry(scope_key).or_insert(0);

        if *current_count >= max_calls {
            return Err(format!("tool `{tool_name}` cannot be called more than {max_calls} times"));
        }

        *current_count += 1;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelToolSource {
    Local,
    Mcp {
        server_name: Option<String>,
        tool_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub output: Value,
    pub context: Value,
}
