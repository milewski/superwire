use super::format::format_tool_name;
use super::request::ChatCompletionRequestMessageExt;
use crate::event::ExecutorEvent;
use crate::model::response::normalize_mcp_tool_result;
use crate::model::types::{ModelRequest, ModelToolSource};
use crate::runtime::ExecutorError;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestToolMessageContent,
};
use serde_json::Value;
use superwire_core::mcp::{McpClient, McpServerConfig};

impl super::OpenAiModelProvider {
    pub(super) fn execute_tool_calls(
        &self,
        request: &ModelRequest,
        tool_calls: &[ChatCompletionMessageToolCall],
    ) -> Result<Vec<Value>, ExecutorError> {
        let mut messages = Vec::new();

        for tool_call in tool_calls {
            let tool_result = self.execute_tool_call(request, tool_call)?;
            let tool_result_text = serde_json::to_string(&tool_result).map_err(|error| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("failed to serialize tool result: {error}"),
            })?;
            let tool_message = ChatCompletionRequestToolMessageArgs::default()
                .tool_call_id(tool_call.id.clone())
                .content(ChatCompletionRequestToolMessageContent::Text(tool_result_text))
                .build()
                .map_err(|error| ExecutorError::Model {
                    agent_name: request.agent_name.clone(),
                    message: format!("failed to build tool result message: {error}"),
                })?;

            messages.push(ChatCompletionRequestMessage::Tool(tool_message).to_json_value(&request.agent_name)?);
        }

        Ok(messages)
    }

    pub(super) fn execute_tool_call(
        &self,
        request: &ModelRequest,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<Value, ExecutorError> {
        let tool_definition = request
            .tools
            .iter()
            .find(|tool_definition| format_tool_name(&tool_definition.name) == tool_call.function.name)
            .ok_or_else(|| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("model requested unknown tool `{}`", tool_call.function.name),
            })?;
        let mut arguments = serde_json::from_str::<Value>(&tool_call.function.arguments).map_err(|error| ExecutorError::Model {
            agent_name: request.agent_name.clone(),
            message: format!("model provided invalid arguments for tool `{}`: {error}", tool_call.function.name),
        })?;

        if let (Some(argument_object), Some(binding_object)) = (arguments.as_object_mut(), tool_definition.bindings.as_object()) {
            for (binding_name, binding_value) in binding_object {
                argument_object.insert(binding_name.clone(), binding_value.clone());
            }
        }

        match &tool_definition.source {
            ModelToolSource::Mcp {
                server_name,
                tool_name,
                endpoint,
                headers,
            } => {
                let server_config = McpServerConfig {
                    name: server_name.clone().unwrap_or_else(|| "default".to_string()),
                    endpoint: endpoint.clone(),
                    headers: headers.clone(),
                };

                request.send_tool_call_started(tool_definition.name.clone(), arguments.clone());

                let result = McpClient::new(server_config)
                    .call_tool(tool_name, arguments)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: error.to_string(),
                    })?;
                let normalized_result = normalize_mcp_tool_result(result);

                request.send_tool_call_completed(tool_definition.name.clone(), normalized_result.clone());

                Ok(normalized_result)
            }
            ModelToolSource::Local => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("tool `{}` is not backed by MCP", tool_definition.name),
            }),
        }
    }
}

trait ToolCallEventSender {
    fn send_tool_call_started(&self, tool_name: String, arguments: Value);

    fn send_tool_call_completed(&self, tool_name: String, result: Value);
}

impl ToolCallEventSender for ModelRequest {
    fn send_tool_call_started(&self, tool_name: String, arguments: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_started(self.agent_name.clone(), tool_name, arguments));
        }
    }

    fn send_tool_call_completed(&self, tool_name: String, result: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_completed(self.agent_name.clone(), tool_name, result));
        }
    }
}
