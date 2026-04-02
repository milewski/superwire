use crate::context::Context;
use crate::error::ProviderError;
use crate::message::{Message, ToolCall};
use crate::traits::{Provider, ProviderResponse, ProviderToolChoice, StopReason, TokenUsage, ToolDefinition};
use crate::AgentConfig;
use async_trait::async_trait;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::options::GenerationOptions;
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
        match message {
            Message::User { content } => Ok(ChatMessage::user(content.clone())),
            Message::Assistant { content } => Ok(ChatMessage::assistant(content.clone())),
            Message::AssistantToolCall { tool: _ } => Ok(ChatMessage::assistant(String::new())),
            Message::ToolResult { result } => Ok(ChatMessage::assistant(result.content().to_string())),
            Message::System { content } => Ok(ChatMessage::system(content.clone())),
        }
    }

    fn build_generation_options(config: &AgentConfig) -> Result<Option<GenerationOptions>, String> {
        let mut generation_options = GenerationOptions::default();
        let mut has_options = false;

        if let Some(temperature) = config.temperature {
            generation_options = generation_options.temperature(temperature);
            has_options = true;
        }

        if let Some(top_p) = config.top_p {
            generation_options = generation_options.top_p(top_p);
            has_options = true;
        }

        if let Some(top_k) = config.top_k {
            generation_options = generation_options.top_k(top_k);
            has_options = true;
        }

        if let Some(repeat_penalty) = config.repeat_penalty {
            generation_options = generation_options.repeat_penalty(repeat_penalty);
            has_options = true;
        }

        if let Some(seed) = config.seed {
            generation_options = generation_options.seed(seed);
            has_options = true;
        }

        if let Some(max_tokens) = config.max_tokens {
            let max_predictions =
                i32::try_from(max_tokens).map_err(|_| format!("max_tokens value {max_tokens} exceeds i32::MAX for Ollama num_predict"))?;

            generation_options = generation_options.num_predict(max_predictions);
            has_options = true;
        }

        if let Some(stop_sequences) = &config.stop_sequences {
            generation_options = generation_options.stop(stop_sequences.clone());
            has_options = true;
        }

        if has_options {
            Ok(Some(generation_options))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn generate(
        &self,
        context: &Context,
        _tools: &[ToolDefinition],
        _tool_choice: ProviderToolChoice,
        config: &AgentConfig,
    ) -> Result<ProviderResponse, ProviderError> {
        let messages: Result<Vec<ChatMessage>, String> = context
            .messages
            .iter()
            .map(|message| self.convert_message_to_ollama(message))
            .collect();

        let messages = messages.map_err(|message| ProviderError::InvalidRequest { message })?;

        let mut request = ChatMessageRequest::new(self.model.clone(), messages);

        if let Some(generation_options) =
            Self::build_generation_options(config).map_err(|message| ProviderError::InvalidRequest { message })?
        {
            request = request.options(generation_options);
        }

        let response = self
            .client
            .send_chat_messages(request)
            .await
            .map_err(|error| ProviderError::Network {
                message: format!("Ollama API error: {error}"),
            })?;

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

        let usage = response.final_data.as_ref().map(|final_data| {
            let input_tokens = usize::from(final_data.prompt_eval_count);
            let output_tokens = usize::from(final_data.eval_count);

            TokenUsage {
                total_tokens: input_tokens + output_tokens,
                input_tokens,
                output_tokens,
            }
        });

        Ok(ProviderResponse {
            tool_calls,
            text,
            stop_reason,
            usage,
        })
    }
}
