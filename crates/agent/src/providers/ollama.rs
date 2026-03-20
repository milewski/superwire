use crate::context::Context;
use crate::message::{Message, MessageRole, ToolCall};
use crate::traits::{Provider, ProviderResponse, StopReason, ToolDefinition};
use async_trait::async_trait;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::Ollama;

pub struct OllamaProvider {
    client: Ollama,
    model: String,
}

impl OllamaProvider {
    pub fn new(host: impl Into<String>, port: u16, model: impl Into<String>) -> Self {
        let client = Ollama::new(host.into(), port);

        Self {
            client,
            model: model.into(),
        }
    }

    fn convert_message_to_ollama(&self, message: &Message) -> Result<ChatMessage, String> {
        match message.role {
            MessageRole::User => Ok(ChatMessage::user(message.content.clone())),
            MessageRole::Assistant => Ok(ChatMessage::assistant(message.content.clone())),
            MessageRole::System => Ok(ChatMessage::system(message.content.clone())),
            MessageRole::Tool | MessageRole::ToolResult => Ok(ChatMessage::assistant(message.content.clone())),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn generate(&self, context: &Context, _tools: &[ToolDefinition]) -> Result<ProviderResponse, String> {
        let messages: Result<Vec<ChatMessage>, String> = context
            .messages
            .iter()
            .map(|message| self.convert_message_to_ollama(message))
            .collect();

        let messages = messages?;

        let request = ChatMessageRequest::new(self.model.clone(), messages);

        let response = self
            .client
            .send_chat_messages(request)
            .await
            .map_err(|error| format!("Ollama API error: {error}"))?;

        let text = Some(response.message.content.clone());

        let tool_calls: Vec<ToolCall> = response
            .message
            .tool_calls
            .iter()
            .filter_map(|tool_call| {
                let serialized = serde_json::to_value(tool_call).ok()?;
                let function = serialized.get("function")?;
                let name = function.get("name")?.as_str()?.to_string();
                let arguments = function.get("arguments")?.clone();

                Some(ToolCall {
                    id: format!("call_{}", uuid::Uuid::new_v4()),
                    name,
                    arguments,
                })
            })
            .collect();

        let stop_reason = if !tool_calls.is_empty() {
            StopReason::ToolCalls
        } else if response.done {
            StopReason::EndOfSequence
        } else {
            StopReason::Other("Unknown".to_string())
        };

        Ok(ProviderResponse {
            tool_calls,
            text,
            stop_reason,
        })
    }
}
