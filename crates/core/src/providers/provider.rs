use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn driver(&self) -> &str;
    fn models(&self) -> &[String];

    async fn execute(
        &self,
        model: &str,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Response>;
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}
