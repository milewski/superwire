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
use jsonschema::ValidationError;
use serde_json::Value;
use superwire_core::mcp::McpServerConfig;

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

        request
            .tool_call_tracker
            .register_call(&tool_definition.name, tool_definition.max_calls, &tool_definition.max_calls_scope)
            .map_err(|message| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message,
            })?;

        log::debug!(
            "processing model tool call: agent={}, requested_tool={}, resolved_tool={}",
            request.agent_name,
            tool_call.function.name,
            tool_definition.name
        );
        let mut arguments = match serde_json::from_str::<Value>(&tool_call.function.arguments) {
            Ok(arguments) => arguments,
            Err(error) => {
                let tool_error = tool_argument_error(
                    &tool_definition.name,
                    format!("tool arguments must be valid JSON: {error}"),
                    &tool_definition.input_schema,
                );

                request.send_tool_call_failed(tool_definition.name.clone(), tool_error.clone());
                log::warn!(
                    "rejected tool call with invalid JSON arguments: agent={}, tool={}, error={}",
                    request.agent_name,
                    tool_definition.name,
                    error
                );

                return Ok(tool_error);
            }
        };

        if let Err(message) = validate_tool_arguments(&arguments, &tool_definition.input_schema) {
            let tool_error = tool_argument_error(&tool_definition.name, message, &tool_definition.input_schema);

            request.send_tool_call_failed(tool_definition.name.clone(), tool_error.clone());
            log::warn!(
                "rejected tool call before MCP dispatch: agent={}, tool={}, error={}",
                request.agent_name,
                tool_definition.name,
                tool_error.get("message").and_then(Value::as_str).unwrap_or("schema mismatch")
            );

            return Ok(tool_error);
        }

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
                log::info!(
                    "dispatching MCP tool call: agent={}, tool={}, mcp_tool={}",
                    request.agent_name,
                    tool_definition.name,
                    tool_name
                );

                let result = request
                    .mcp_pool
                    .get(&server_config)?
                    .call_tool(tool_name, arguments)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: error.to_string(),
                    })?;
                let normalized_result = normalize_mcp_tool_result(result);

                request.send_tool_call_completed(tool_definition.name.clone(), normalized_result.clone());
                log::debug!(
                    "completed MCP tool call: agent={}, tool={}",
                    request.agent_name,
                    tool_definition.name
                );

                Ok(normalized_result)
            }
            ModelToolSource::Local => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("tool `{}` is not backed by MCP", tool_definition.name),
            }),
        }
    }
}

fn validate_tool_arguments(arguments: &Value, schema: &Value) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema).map_err(|error| format!("tool schema could not be compiled: {error}"))?;
    let mut validation_issues = validator.iter_errors(arguments).map(format_validation_issue).collect::<Vec<_>>();

    if validation_issues.is_empty() {
        return Ok(());
    }

    validation_issues.sort();
    validation_issues.dedup();

    Err(format!(
        "tool arguments do not match the declared schema: {}. Correct the arguments and call the tool again.",
        validation_issues.join("; ")
    ))
}

fn tool_argument_error(tool_name: &str, message: String, schema: &Value) -> Value {
    serde_json::json!({
        "error": "tool_argument_schema_mismatch",
        "tool_name": tool_name,
        "message": message,
        "expected_schema": schema,
    })
}

fn format_validation_issue(validation_error: ValidationError<'_>) -> String {
    let instance_path = normalize_instance_path(&validation_error.instance_path().to_string());

    if instance_path == "$" {
        return validation_error.to_string();
    }

    format!("{instance_path}: {validation_error}")
}

fn normalize_instance_path(instance_path: &str) -> String {
    if instance_path.is_empty() {
        return "$".to_string();
    }

    let mut normalized_path = String::from("$");

    for path_segment in instance_path.trim_start_matches('/').split('/') {
        if path_segment.is_empty() {
            continue;
        }

        if path_segment.chars().all(|character| character.is_ascii_digit()) {
            normalized_path.push('[');
            normalized_path.push_str(path_segment);
            normalized_path.push(']');

            continue;
        }

        normalized_path.push('.');
        normalized_path.push_str(path_segment);
    }

    normalized_path
}

trait ToolCallEventSender {
    fn send_tool_call_started(&self, tool_name: String, arguments: Value);

    fn send_tool_call_failed(&self, tool_name: String, error: Value);

    fn send_tool_call_completed(&self, tool_name: String, result: Value);
}

impl ToolCallEventSender for ModelRequest {
    fn send_tool_call_started(&self, tool_name: String, arguments: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_started(self.agent_name.clone(), tool_name, arguments));
        }
    }

    fn send_tool_call_failed(&self, tool_name: String, error: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_failed(self.agent_name.clone(), tool_name, error));
        }
    }

    fn send_tool_call_completed(&self, tool_name: String, result: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_completed(self.agent_name.clone(), tool_name, result));
        }
    }
}
