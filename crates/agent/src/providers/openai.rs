use crate::context::Context;
use crate::message::{Message, MessageRole, ToolCall};
use crate::traits::{Provider, ProviderResponse, StopReason, ToolDefinition};
use crate::AgentConfig;
use async_openai::types::{ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs, ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, FunctionObjectArgs, ReasoningEffort};
use async_openai::Client;
use async_trait::async_trait;
use serde_json::json;

pub struct OpenAIProvider {
    client: Client<async_openai::config::OpenAIConfig>,
    model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let config = async_openai::config::OpenAIConfig::new().with_api_key(api_key.into());
        let client = Client::with_config(config);

        Self {
            client,
            model: model.into(),
        }
    }

    pub fn new_with_base_url(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let config = async_openai::config::OpenAIConfig::new()
            .with_api_key(api_key.into())
            .with_api_base(base_url.into());
        let client = Client::with_config(config);

        Self {
            client,
            model: model.into(),
        }
    }

    pub fn new_local(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new_with_base_url(base_url, String::new(), model)
    }

    fn convert_message_to_openai(&self, message: &Message) -> Result<ChatCompletionRequestMessage, String> {
        match message.role {
            MessageRole::User => {
                let user_message = ChatCompletionRequestUserMessageArgs::default()
                    .content(message.content.clone())
                    .build()
                    .map_err(|error| format!("Failed to build user message: {error}"))?;
                Ok(ChatCompletionRequestMessage::User(user_message))
            }
            MessageRole::Assistant => {
                let mut assistant_message_builder = ChatCompletionRequestAssistantMessageArgs::default();
                assistant_message_builder.content(message.content.clone());

                if let Some(tool_call) = &message.tool_call {
                    let openai_tool_call = ChatCompletionMessageToolCall {
                        id: tool_call.id.clone(),
                        r#type: ChatCompletionToolType::Function,
                        function: FunctionCall {
                            name: tool_call.name.clone(),
                            arguments: tool_call.arguments.to_string(),
                        },
                    };

                    assistant_message_builder.tool_calls(vec![openai_tool_call]);
                }

                let assistant_message = assistant_message_builder
                    .build()
                    .map_err(|error| format!("Failed to build assistant message: {error}"))?;
                Ok(ChatCompletionRequestMessage::Assistant(assistant_message))
            }
            MessageRole::System => {
                let system_message = ChatCompletionRequestSystemMessageArgs::default()
                    .content(message.content.clone())
                    .build()
                    .map_err(|error| format!("Failed to build system message: {error}"))?;
                Ok(ChatCompletionRequestMessage::System(system_message))
            }
            MessageRole::Tool | MessageRole::ToolResult => {
                if let Some(tool_result) = &message.tool_result {
                    let tool_message = ChatCompletionRequestToolMessageArgs::default()
                        .content(message.content.clone())
                        .tool_call_id(tool_result.tool_call_id.clone())
                        .build()
                        .map_err(|error| format!("Failed to build tool message: {error}"))?;
                    Ok(ChatCompletionRequestMessage::Tool(tool_message))
                } else {
                    Err("Tool message missing tool_result".to_string())
                }
            }
        }
    }

    fn convert_tools_to_openai(&self, tools: &[ToolDefinition]) -> Result<Vec<ChatCompletionTool>, String> {
        let mut openai_tools: Vec<ChatCompletionTool> = Vec::new();

        for tool in tools {
            let parameters = serde_json::to_value(&tool.parameters_schema)
                .map_err(|error| format!("Failed to serialize schema for '{}': {error}", tool.name))?;

            let function = FunctionObjectArgs::default()
                .name(&tool.name)
                .description(&tool.description)
                .parameters(parameters)
                .build()
                .map_err(|error| format!("Failed to build function object for '{}': {error}", tool.name))?;

            let openai_tool = ChatCompletionToolArgs::default()
                .r#type(ChatCompletionToolType::Function)
                .function(function)
                .build()
                .map_err(|error| format!("Failed to build tool '{}': {error}", tool.name))?;

            openai_tools.push(openai_tool);
        }

        Ok(openai_tools)
    }

    fn convert_stop_reason(finish_reason: Option<async_openai::types::FinishReason>) -> StopReason {
        match finish_reason {
            Some(async_openai::types::FinishReason::Stop) => StopReason::EndOfSequence,
            Some(async_openai::types::FinishReason::Length) => StopReason::MaxTokens,
            Some(async_openai::types::FinishReason::ToolCalls) => StopReason::ToolCalls,
            Some(async_openai::types::FinishReason::ContentFilter) => StopReason::ContentFilter,
            Some(async_openai::types::FinishReason::FunctionCall) => StopReason::ToolCalls,
            None => StopReason::Other("No finish reason provided".to_string()),
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<ProviderResponse, String> {
        let messages: Result<Vec<ChatCompletionRequestMessage>, String> = context
            .messages
            .iter()
            .map(|message| self.convert_message_to_openai(message))
            .collect();

        let messages = messages?;

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.model);
        request_builder.messages(messages);
        request_builder.parallel_tool_calls(true);

        if let Some(temperature) = config.temperature {
            request_builder.temperature(temperature);
        }

        if !tools.is_empty() {
            request_builder.tools(self.convert_tools_to_openai(tools)?);
        }

        let request = request_builder
            .build()
            .map_err(|error| format!("Failed to build request: {error}"))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|error| format!("OpenAI API error: {error}"))?;

        let choice = response.choices.first().ok_or_else(|| "No choices in response".to_string())?;

        let stop_reason = Self::convert_stop_reason(choice.finish_reason);
        let text = choice.message.content.clone();

        let tool_calls = if let Some(openai_tool_calls) = &choice.message.tool_calls {
            openai_tool_calls
                .iter()
                .map(|tool_call| {
                    let arguments: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap_or_else(|_| json!({}));

                    ToolCall {
                        id: tool_call.id.clone(),
                        name: tool_call.function.name.clone(),
                        arguments,
                    }
                })
                .collect()
        } else {
            vec![]
        };

        Ok(ProviderResponse {
            tool_calls,
            text,
            stop_reason,
        })
    }
}
