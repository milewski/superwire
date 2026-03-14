use crate::ast::Agent;
use crate::providers::error::ProviderError;
use crate::providers::provider::{AgentOutput, Message, Provider, ToolCall, ToolDefinition};
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs, ChatCompletionTool,
    ChatCompletionToolArgs, ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObjectArgs,
};
use async_openai::Client;
use serde_json::Value;

pub struct OpenAiProvider {
    name: String,
    models: Vec<String>,
    client: Client<OpenAIConfig>,
}

impl OpenAiProvider {
    #[must_use]
    pub fn new(name: String, api_key: String, models: Vec<String>) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self { name, models, client }
    }

    #[must_use]
    pub fn with_endpoint(name: String, api_key: String, endpoint: String, models: Vec<String>) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key).with_api_base(endpoint);
        let client = Client::with_config(config);

        Self { name, models, client }
    }

    fn convert_messages_to_openai_format(
        messages: &[Message],
    ) -> Result<Vec<ChatCompletionRequestMessage>, ProviderError> {
        messages
            .iter()
            .map(|message| match message {
                Message::System { content } => ChatCompletionRequestSystemMessageArgs::default()
                    .content(content.clone())
                    .build()
                    .map(ChatCompletionRequestMessage::System)
                    .map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to build system message: {error}"),
                    }),
                Message::User { content } => ChatCompletionRequestUserMessageArgs::default()
                    .content(content.clone())
                    .build()
                    .map(ChatCompletionRequestMessage::User)
                    .map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to build user message: {error}"),
                    }),
                Message::Assistant { content, tool_calls } => {
                    let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

                    builder.content(ChatCompletionRequestAssistantMessageContent::Text(content.clone()));

                    if let Some(calls) = tool_calls {
                        let openai_tool_calls: Vec<ChatCompletionMessageToolCall> = calls
                            .iter()
                            .map(|call| ChatCompletionMessageToolCall {
                                id: call.id.clone(),
                                r#type: ChatCompletionToolType::Function,
                                function: async_openai::types::FunctionCall {
                                    name: call.name.clone(),
                                    arguments: call.arguments.clone(),
                                },
                            })
                            .collect();

                        builder.tool_calls(openai_tool_calls);
                    }

                    builder
                        .build()
                        .map(ChatCompletionRequestMessage::Assistant)
                        .map_err(|error| ProviderError::InvalidInput {
                            message: format!("Failed to build assistant message: {error}"),
                        })
                }
                Message::Tool { tool_call_id, content } => ChatCompletionRequestToolMessageArgs::default()
                    .content(content.clone())
                    .tool_call_id(tool_call_id.clone())
                    .build()
                    .map(ChatCompletionRequestMessage::Tool)
                    .map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to build tool message: {error}"),
                    }),
            })
            .collect()
    }

    fn build_openai_tools(tools: &[ToolDefinition]) -> Result<Vec<ChatCompletionTool>, ProviderError> {
        tools
            .iter()
            .map(|tool| {
                let function = FunctionObjectArgs::default()
                    .name(tool.name.clone())
                    .description(tool.description.clone())
                    .parameters(tool.parameters_schema.clone())
                    .build()
                    .map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to build function object: {error}"),
                    })?;

                ChatCompletionToolArgs::default()
                    .r#type(ChatCompletionToolType::Function)
                    .function(function)
                    .build()
                    .map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to build chat completion tool: {error}"),
                    })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    async fn execute_agent(
        &self,
        agent: &Agent,
        context: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<AgentOutput, ProviderError> {
        log::debug!("OpenAiProvider executing agent: {}", agent.name);

        let openai_messages = Self::convert_messages_to_openai_format(&context)?;
        log::trace!("Converted {} messages to OpenAI format", openai_messages.len());

        let model_name = agent
            .properties
            .iter()
            .find_map(|prop| {
                if let crate::ast::AgentProperty::Model {
                    value: crate::ast::Value::String(model_ref),
                    ..
                } = prop
                {
                    model_ref.split('/').nth(1).map(std::string::ToString::to_string)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "gpt-4o".to_string());

        log::info!("Using model: {model_name}");

        let openai_tools = Self::build_openai_tools(&tools)?;
        log::debug!("Configured {} tools for OpenAI", openai_tools.len());

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(model_name.clone()).messages(openai_messages);

        if !openai_tools.is_empty() {
            request_builder.tools(openai_tools);
        }

        let request = request_builder.build().map_err(|error| {
            log::error!("Failed to build OpenAI request: {error}");
            ProviderError::InvalidInput {
                message: format!("Failed to build OpenAI request: {error}"),
            }
        })?;

        log::debug!("Sending request to OpenAI");

        let response = self.client.chat().create(request).await.map_err(|error| {
            log::error!("OpenAI API request failed: {error}");
            ProviderError::ApiError {
                message: format!("OpenAI API request failed: {error}"),
                status_code: None,
                suggestion: Some("Check your API key and network connection".to_string()),
            }
        })?;

        log::debug!("Received successful response from OpenAI");

        let choice = response.choices.first().ok_or_else(|| {
            log::error!("OpenAI response contained no choices");
            ProviderError::ApiError {
                message: "OpenAI response contained no choices".to_string(),
                status_code: None,
                suggestion: None,
            }
        })?;

        let output_content = choice.message.content.clone().unwrap_or_default();

        log::debug!("OpenAI response content length: {} chars", output_content.len());

        let tool_calls = choice.message.tool_calls.as_ref().map(|calls| {
            log::debug!("OpenAI response contains {} tool calls", calls.len());
            calls
                .iter()
                .enumerate()
                .map(|(index, call)| {
                    log::trace!("Tool call {}: {}", index, call.function.name);
                    ToolCall {
                        id: call.id.clone(),
                        name: call.function.name.clone(),
                        arguments: call.function.arguments.clone(),
                    }
                })
                .collect()
        });

        let mut updated_context = Vec::with_capacity(context.len() + 1);
        updated_context.extend_from_slice(&context);
        updated_context.push(Message::Assistant {
            content: output_content.clone(),
            tool_calls,
        });

        log::info!("OpenAiProvider execution completed successfully");

        Ok(AgentOutput {
            output: Value::String(output_content),
            context: updated_context,
        })
    }
}

pub struct OpenAiProviderBuilder;

impl crate::providers::builder::ProviderBuilder for OpenAiProviderBuilder {
    fn build(&self, provider: &crate::ast::Provider) -> Result<crate::providers::provider::ProviderRef, ProviderError> {
        // Try to get api_key from config first, then fall back to environment variable
        let api_key = provider
            .config
            .get("api_key")
            .and_then(|v| match v {
                crate::ast::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| ProviderError::InvalidInput {
                message: "API key not found. Provide it in config { api_key: \"...\" } or set OPENAI_API_KEY environment variable".to_string(),
            })?;

        // Check if custom endpoint is provided
        let endpoint = provider.config.get("endpoint").and_then(|v| match v {
            crate::ast::Value::String(s) => Some(s.clone()),
            _ => None,
        });

        let openai_provider = if let Some(endpoint_url) = endpoint {
            OpenAiProvider::with_endpoint(provider.name.clone(), api_key, endpoint_url, provider.models.clone())
        } else {
            OpenAiProvider::new(provider.name.clone(), api_key, provider.models.clone())
        };

        Ok(std::sync::Arc::new(openai_provider))
    }

    fn driver_name(&self) -> &'static str {
        "openai"
    }
}
