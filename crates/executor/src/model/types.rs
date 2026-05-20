use crate::event::ExecutorEvent;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::provider::ProviderConfig;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub agent_name: String,
    pub provider_config: ProviderConfig,
    pub model_name: String,
    pub inference: BTreeMap<String, Value>,
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

impl ModelToolDefinition {
    const FINALIZE_NAME: &'static str = "finalize";
    const INTERNAL_FINALIZE_DISPLAY_NAME: &'static str = "internal:finalize";

    #[must_use]
    pub fn finalize(output_schema: Value) -> Self {
        Self {
            name: Self::FINALIZE_NAME.to_string(),
            description: Some(
                "Finish the agent run. Use success with output matching the schema, or fail with a clear reason when the request cannot be fulfilled."
                    .to_string(),
            ),
            source: ModelToolSource::Finalize,
            input_schema: finalize_tool_schema(output_schema),
            output_schema: serde_json::json!({ "type": "object" }),
            bindings: Value::Null,
            max_calls: None,
            max_calls_scope: ToolCallLimitScope::Workflow,
        }
    }

    #[must_use]
    pub fn event_display_name(&self) -> String {
        match &self.source {
            ModelToolSource::Finalize => Self::INTERNAL_FINALIZE_DISPLAY_NAME.to_string(),
            _ => self.name.clone(),
        }
    }
}

fn finalize_tool_schema(output_schema: Value) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "type": {
                "type": "string",
                "enum": ["success", "fail"],
            },
            "output": output_schema,
            "reason": {
                "type": "string",
                "minLength": 1,
            },
        },
        "required": ["type"],
        "additionalProperties": false,
        "oneOf": [
            {
                "properties": {
                    "type": { "const": "success" },
                },
                "required": ["type", "output"],
                "not": { "required": ["reason"] },
            },
            {
                "properties": {
                    "type": { "const": "fail" },
                },
                "required": ["type", "reason"],
                "not": { "required": ["output"] },
            },
        ],
    })
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
    Finalize,
    Local,
    Mcp {
        server_name: Option<String>,
        tool_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
    McpPrompt {
        server_name: String,
        prompt_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
    McpResource {
        server_name: String,
        resource_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub output: Value,
    pub context: Value,
}
