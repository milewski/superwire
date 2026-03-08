use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::tools::{ToolFunctionInfo, ToolInfo, ToolType};
use ollama_rs::Ollama;
use schemars::Schema;
use serde_json::Value;

use crate::providers::error::ProviderError;
use crate::providers::provider::{Provider, ProviderModelConfig, ProviderRequest, ProviderResponse};

#[derive(Debug, Default)]
pub struct OllamaProvider;

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    async fn chat(
        &self,
        model: &ProviderModelConfig,
        request: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        let ollama = build_client(model)?;

        let tools = request
            .tools
            .iter()
            .map(|tool_def| {
                let parameters = match &tool_def.input_schema {
                    Value::Object(map) => Schema::from(map.clone()),
                    _ => Schema::from(serde_json::Map::new()),
                };

                ToolInfo {
                    tool_type: ToolType::Function,
                    function: ToolFunctionInfo {
                        name: tool_def.name.clone(),
                        description: tool_def.description.clone(),
                        parameters,
                    },
                }
            })
            .collect();

        let chat_request = ChatMessageRequest::new(
            model.model_name.clone(),
            vec![ChatMessage::user(request.prompt.clone())],
        )
        .tools(tools);

        let response =
            ollama
                .send_chat_messages(chat_request)
                .await
                .map_err(|source| ProviderError::RequestFailed {
                    message: source.to_string(),
                })?;

        Ok(ProviderResponse {
            message: response.message.content,
            tool_calls: response
                .message
                .tool_calls
                .into_iter()
                .map(|call| crate::providers::provider::ToolCall {
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
        })
    }

    fn driver(&self) -> &'static str {
        "ollama"
    }
}

fn build_client(model: &ProviderModelConfig) -> Result<Ollama, ProviderError> {
    let endpoint = model.api_endpoint.as_deref().unwrap_or("http://127.0.0.1:11434");
    Ollama::try_new(endpoint).map_err(|source| ProviderError::RequestFailed {
        message: source.to_string(),
    })
}
