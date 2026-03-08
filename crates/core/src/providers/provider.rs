use crate::ast::Agent;
use crate::providers::error::ProviderError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: String,
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub output: Value,
    pub context: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
}

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;

    fn models(&self) -> &[String];

    async fn execute_agent(
        &self,
        agent: &Agent,
        context: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<AgentOutput, ProviderError>;
}

pub type ProviderRef = Arc<dyn Provider>;
