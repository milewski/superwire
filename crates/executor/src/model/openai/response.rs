use crate::runtime::ExecutorError;
use async_openai::types::ChatCompletionMessageToolCall;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionChoice {
    message: OpenAiChatCompletionMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ChatCompletionMessageToolCall>>,
    reasoning_content: Option<String>,
}

pub(super) trait ChatCompletionResponseExt {
    fn extract_assistant_content(&self) -> Option<String>;

    fn extract_tool_calls(&self) -> Option<Vec<ChatCompletionMessageToolCall>>;

    fn extract_tool_call_message(&self) -> Result<Value, ExecutorError>;
}

impl ChatCompletionResponseExt for OpenAiChatCompletionResponse {
    fn extract_assistant_content(&self) -> Option<String> {
        self.choices
            .iter()
            .filter_map(|choice| choice.message.content.as_deref())
            .map(str::trim)
            .find(|content| !content.is_empty())
            .map(str::to_string)
    }

    fn extract_tool_calls(&self) -> Option<Vec<ChatCompletionMessageToolCall>> {
        self.choices.iter().find_map(|choice| choice.message.extract_tool_calls())
    }

    fn extract_tool_call_message(&self) -> Result<Value, ExecutorError> {
        let Some(message) = self
            .choices
            .iter()
            .map(|choice| &choice.message)
            .find(|message| message.has_tool_calls())
        else {
            return Err(ExecutorError::Model {
                agent_name: "unknown".to_string(),
                message: "model response did not include assistant tool calls".to_string(),
            });
        };

        Ok(message.to_assistant_request_message())
    }
}

impl OpenAiChatCompletionMessage {
    fn extract_tool_calls(&self) -> Option<Vec<ChatCompletionMessageToolCall>> {
        let tool_calls = self.tool_calls.clone()?;

        (!tool_calls.is_empty()).then_some(tool_calls)
    }

    fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().is_some_and(|tool_calls| !tool_calls.is_empty())
    }

    fn to_assistant_request_message(&self) -> Value {
        let mut request_message = serde_json::json!({
            "role": "assistant",
            "tool_calls": self.tool_calls.clone(),
        });

        if let Some(content) = &self.content {
            request_message["content"] = Value::String(content.clone());
        }

        if let Some(reasoning_content) = &self.reasoning_content {
            request_message["reasoning_content"] = Value::String(reasoning_content.clone());
        }

        request_message
    }
}
