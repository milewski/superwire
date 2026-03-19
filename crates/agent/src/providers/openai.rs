use crate::context::Context;
use crate::message::{Message, MessageRole, ToolCall};
use crate::traits::{Provider, ProviderResponse, StopReason, Tool};
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs, ChatCompletionTool,
    ChatCompletionToolArgs, ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObjectArgs,
};
use async_openai::Client;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct OpenAIProvider<T: Tool> {
    client: Client<async_openai::config::OpenAIConfig>,
    model: String,
    phantom: std::marker::PhantomData<T>,
}

impl<T: Tool> OpenAIProvider<T> {
    pub fn new(api_key: String, model: String) -> Self {
        let config = async_openai::config::OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            model,
            phantom: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn with_base_url(self, base_url: String, api_key: String) -> Self {
        let config = async_openai::config::OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url);
        let client = Client::with_config(config);
        Self {
            client,
            model: self.model,
            phantom: std::marker::PhantomData,
        }
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
                let assistant_message = ChatCompletionRequestAssistantMessageArgs::default()
                    .content(message.content.clone())
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

    fn convert_tools_to_openai(&self, tools: &[Arc<T>]) -> Result<Vec<ChatCompletionTool>, String> {
        tools
            .iter()
            .map(|tool| {
                let function = FunctionObjectArgs::default()
                    .name(tool.name())
                    .description(tool.description())
                    .parameters(json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    }))
                    .build()
                    .map_err(|error| format!("Failed to build function object: {error}"))?;

                ChatCompletionToolArgs::default()
                    .r#type(ChatCompletionToolType::Function)
                    .function(function)
                    .build()
                    .map_err(|error| format!("Failed to build tool: {error}"))
            })
            .collect()
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
impl<T: Tool + Send + Sync> Provider for OpenAIProvider<T> {
    type Input = String;
    type Tool = T;

    async fn generate(&self, context: &Context<Self::Input, Self::Tool>) -> Result<ProviderResponse, String> {
        let messages: Result<Vec<ChatCompletionRequestMessage>, String> = context
            .messages
            .iter()
            .map(|message| self.convert_message_to_openai(message))
            .collect();

        let messages = messages?;

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.model).messages(messages);

        if !context.tools.is_empty() {
            let tools = self.convert_tools_to_openai(&context.tools)?;
            request_builder.tools(tools);
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

        let choice = response
            .choices
            .first()
            .ok_or_else(|| "No choices in response".to_string())?;

        let stop_reason = Self::convert_stop_reason(choice.finish_reason);
        let text = choice.message.content.clone();

        let tool_calls = if let Some(openai_tool_calls) = &choice.message.tool_calls {
            openai_tool_calls
                .iter()
                .map(|tool_call| {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&tool_call.function.arguments).unwrap_or_else(|_| json!({}));

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
