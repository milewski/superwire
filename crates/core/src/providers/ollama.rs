use crate::ast::Agent;
use crate::providers::error::ProviderError;
use crate::providers::provider::{AgentOutput, Message, Provider, ToolDefinition};
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::Ollama as OllamaClient;
use serde_json::Value;

pub struct OllamaProvider {
    name: String,
    client: OllamaClient,
    models: Vec<String>,
}

impl OllamaProvider {
    pub fn new(name: String, api_endpoint: String, models: Vec<String>) -> Self {
        let client = OllamaClient::new(api_endpoint, 11434);

        Self { name, client, models }
    }
}

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    async fn execute_agent(
        &self,
        _agent: &Agent,
        context: Vec<Message>,
        _tools: Vec<ToolDefinition>,
    ) -> Result<AgentOutput, ProviderError> {
        let mut messages = Vec::new();

        for message in &context {
            match message {
                Message::User { content } => {
                    messages.push(ChatMessage::user(content.clone()));
                }
                Message::Assistant { content, .. } => {
                    messages.push(ChatMessage::assistant(content.clone()));
                }
                Message::Tool { content, .. } => {
                    messages.push(ChatMessage::user(content.clone()));
                }
            }
        }

        let model_name = "qwen3:8b".to_string();

        let request = ChatMessageRequest::new(model_name, messages);

        let response = self
            .client
            .send_chat_messages(request)
            .await
            .map_err(|error| ProviderError::ApiError {
                message: format!("Ollama API error: {}", error),
                status_code: None,
                suggestion: Some("Check that Ollama server is running and accessible".to_string()),
            })?;

        let output_content = response.message.content.clone();

        let mut updated_context = context.clone();

        updated_context.push(Message::Assistant {
            content: output_content.clone(),
            tool_calls: None,
        });

        Ok(AgentOutput {
            output: Value::String(output_content),
            context: updated_context,
        })
    }
}
