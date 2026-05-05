use crate::event::ExecutorEvent;
use serde_json::Value;
use std::collections::BTreeMap;
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub source: ModelToolSource,
    pub input_schema: Value,
    pub output_schema: Value,
    pub bindings: Value,
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
