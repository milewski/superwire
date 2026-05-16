use super::format::format_tool_name;
use super::request::ChatCompletionRequestMessageExt;
use crate::event::{ExecutorEvent, McpCallEventDetails};
use crate::model::response::normalize_mcp_tool_result;
use crate::model::types::{ModelRequest, ModelToolDefinition, ModelToolSource};
use crate::runtime::ExecutorError;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestToolMessageContent,
};
use jsonschema::ValidationError;
use serde_json::Value;
use std::time::{Duration, Instant};
use superwire_core::mcp::McpServerConfig;

impl super::OpenAiModelProvider {
    pub(super) fn execute_tool_calls(
        &self,
        request: &ModelRequest,
        tool_calls: &[ChatCompletionMessageToolCall],
    ) -> Result<ToolCallRound, ExecutorError> {
        let mut messages = Vec::new();

        for tool_call in tool_calls {
            let tool_outcome = self.execute_tool_call(request, tool_call)?;

            if let ToolCallOutcome::Finalized(finalize_result) = tool_outcome {
                return Ok(ToolCallRound {
                    messages,
                    finalize_result: Some(finalize_result),
                });
            }

            let ToolCallOutcome::Continue(tool_result) = tool_outcome else {
                unreachable!("finalize outcome should return above");
            };
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

        Ok(ToolCallRound {
            messages,
            finalize_result: None,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn execute_tool_call(
        &self,
        request: &ModelRequest,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        let tool_definition = request
            .tools
            .iter()
            .find(|tool_definition| format_tool_name(&tool_definition.name) == tool_call.function.name)
            .ok_or_else(|| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("model requested unknown tool `{}`", tool_call.function.name),
            })?;
        let tool_call_started_at = Instant::now();

        if let Some(tool_error) = request.call_limit_error(tool_definition) {
            return Ok(ToolCallOutcome::Continue(tool_error));
        }

        log::debug!(
            "processing model tool call: agent={}, requested_tool={}, resolved_tool={}",
            request.agent_name,
            tool_call.function.name,
            tool_definition.name
        );
        let mut arguments = match serde_json::from_str::<Value>(&tool_call.function.arguments) {
            Ok(arguments) => arguments,
            Err(error) => {
                let tool_error = tool_definition.argument_error(format!("tool arguments must be valid JSON: {error}"));

                if !matches!(tool_definition.source, ModelToolSource::Finalize) {
                    request.send_tool_call_failed(tool_definition.name.clone(), tool_error.clone(), tool_call_started_at.elapsed());
                }
                log::warn!(
                    "rejected tool call with invalid JSON arguments: agent={}, tool={}, error={}",
                    request.agent_name,
                    tool_definition.name,
                    error
                );

                return Ok(ToolCallOutcome::Continue(tool_error));
            }
        };

        let validation_started_at = Instant::now();

        if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
            request.send_mcp_tool_validation_started(
                tool_definition.name.clone(),
                arguments.clone(),
                tool_definition.input_schema.clone(),
            );
        }

        if let Err(message) = validate_tool_arguments(&arguments, &tool_definition.input_schema) {
            let tool_error = tool_definition.argument_error(message);

            if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
                request.send_mcp_tool_validation_failed(tool_definition.name.clone(), tool_error.clone(), validation_started_at.elapsed());
            }

            if !matches!(tool_definition.source, ModelToolSource::Finalize) {
                request.send_tool_call_failed(tool_definition.name.clone(), tool_error.clone(), tool_call_started_at.elapsed());
            }
            log::warn!(
                "rejected tool call before MCP dispatch: agent={}, tool={}, error={}",
                request.agent_name,
                tool_definition.name,
                tool_error.get("message").and_then(Value::as_str).unwrap_or("schema mismatch")
            );

            return Ok(ToolCallOutcome::Continue(tool_error));
        }

        if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
            request.send_mcp_tool_validation_completed(tool_definition.name.clone(), validation_started_at.elapsed());
        }

        if matches!(tool_definition.source, ModelToolSource::Finalize) {
            return tool_definition.parse_finalize_arguments(arguments).map(ToolCallOutcome::Finalized);
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
                let call_details = McpCallEventDetails::new(
                    "call".to_string(),
                    tool_definition.name.clone(),
                    server_config.name.clone(),
                    tool_name.clone(),
                    arguments.clone(),
                    Some(tool_definition.input_schema.clone()),
                );

                request.send_tool_call_started(tool_definition.name.clone(), arguments.clone());
                request.send_mcp_call_started(call_details.clone());
                let started_at = Instant::now();
                log::info!(
                    "dispatching MCP tool call: agent={}, tool={}, mcp_tool={}",
                    request.agent_name,
                    tool_definition.name,
                    tool_name
                );

                let result = match request.mcp_pool.get(&server_config)?.call_tool(tool_name, arguments) {
                    Ok(result) => result,
                    Err(error) => {
                        request.send_mcp_call_failed(call_details, Value::String(error.to_string()), started_at.elapsed());

                        return Err(ExecutorError::Model {
                            agent_name: request.agent_name.clone(),
                            message: error.to_string(),
                        });
                    }
                };
                let normalized_result = normalize_mcp_tool_result(result.clone());

                request.send_mcp_call_completed(call_details, normalized_result.clone(), result, started_at.elapsed());
                request.send_tool_call_completed(tool_definition.name.clone(), normalized_result.clone(), started_at.elapsed());
                log::debug!(
                    "completed MCP tool call: agent={}, tool={}",
                    request.agent_name,
                    tool_definition.name
                );

                Ok(ToolCallOutcome::Continue(normalized_result))
            }
            ModelToolSource::McpPrompt {
                server_name,
                prompt_name,
                endpoint,
                headers,
            } => {
                let server_config = McpServerConfig {
                    name: server_name.clone(),
                    endpoint: endpoint.clone(),
                    headers: headers.clone(),
                };
                let call_details = McpCallEventDetails::new(
                    "render".to_string(),
                    tool_definition.name.clone(),
                    server_config.name.clone(),
                    prompt_name.clone(),
                    arguments.clone(),
                    Some(tool_definition.input_schema.clone()),
                );

                request.send_tool_call_started(tool_definition.name.clone(), arguments.clone());
                request.send_mcp_call_started(call_details.clone());
                let started_at = Instant::now();
                let result = match request.mcp_pool.get(&server_config)?.get_prompt(prompt_name, arguments) {
                    Ok(result) => result,
                    Err(error) => {
                        request.send_mcp_call_failed(call_details, Value::String(error.to_string()), started_at.elapsed());

                        return Err(ExecutorError::Model {
                            agent_name: request.agent_name.clone(),
                            message: error.to_string(),
                        });
                    }
                };
                let rendered_result = Value::String(render_mcp_prompt_result(&result));

                request.send_mcp_call_completed(call_details, rendered_result.clone(), result, started_at.elapsed());
                request.send_tool_call_completed(tool_definition.name.clone(), rendered_result.clone(), started_at.elapsed());

                Ok(ToolCallOutcome::Continue(rendered_result))
            }
            ModelToolSource::McpResource {
                server_name,
                resource_name,
                endpoint,
                headers,
            } => {
                let server_config = McpServerConfig {
                    name: server_name.clone(),
                    endpoint: endpoint.clone(),
                    headers: headers.clone(),
                };
                let call_details = McpCallEventDetails::new(
                    "read".to_string(),
                    tool_definition.name.clone(),
                    server_config.name.clone(),
                    resource_name.clone(),
                    arguments.clone(),
                    Some(tool_definition.input_schema.clone()),
                );

                request.send_tool_call_started(tool_definition.name.clone(), arguments.clone());
                request.send_mcp_call_started(call_details.clone());
                let started_at = Instant::now();
                let result = match request.mcp_pool.get(&server_config)?.read_resource(resource_name, arguments) {
                    Ok(result) => result,
                    Err(error) => {
                        request.send_mcp_call_failed(call_details, Value::String(error.to_string()), started_at.elapsed());

                        return Err(ExecutorError::Model {
                            agent_name: request.agent_name.clone(),
                            message: error.to_string(),
                        });
                    }
                };
                let rendered_result = Value::String(render_mcp_resource_result(&result));

                request.send_mcp_call_completed(call_details, rendered_result.clone(), result, started_at.elapsed());
                request.send_tool_call_completed(tool_definition.name.clone(), rendered_result.clone(), started_at.elapsed());

                Ok(ToolCallOutcome::Continue(rendered_result))
            }
            ModelToolSource::Finalize => unreachable!("finalize tool calls should return before MCP dispatch"),
            ModelToolSource::Local => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("tool `{}` is not backed by MCP", tool_definition.name),
            }),
        }
    }
}

fn render_mcp_prompt_result(result: &Value) -> String {
    result
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message.pointer("/content/text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| result.to_string())
}

fn render_mcp_resource_result(result: &Value) -> String {
    result
        .get("contents")
        .and_then(Value::as_array)
        .map(|contents| {
            contents
                .iter()
                .filter_map(|content| content.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| result.to_string())
}

pub(super) struct ToolCallRound {
    pub(super) messages: Vec<Value>,
    pub(super) finalize_result: Option<FinalizeResult>,
}

pub(super) enum ToolCallOutcome {
    Continue(Value),
    Finalized(FinalizeResult),
}

pub(super) enum FinalizeResult {
    Success(Value),
    Fail(String),
}

impl ModelToolDefinition {
    fn parse_finalize_arguments(&self, arguments: Value) -> Result<FinalizeResult, ExecutorError> {
        match arguments.get("type").and_then(Value::as_str) {
            Some("success") => Ok(FinalizeResult::Success(arguments.get("output").cloned().unwrap_or(Value::Null))),
            Some("fail") => Ok(FinalizeResult::Fail(
                arguments
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("agent failed without a reason")
                    .to_string(),
            )),
            _ => Err(ExecutorError::Model {
                agent_name: "unknown".to_string(),
                message: "validated finalize arguments did not include a supported type".to_string(),
            }),
        }
    }

    fn argument_error(&self, message: String) -> Value {
        serde_json::json!({
            "error": "tool_argument_schema_mismatch",
            "tool_name": self.name,
            "message": message,
            "expected_schema": self.input_schema,
        })
    }

    fn call_limit_error(&self, message: String) -> Value {
        serde_json::json!({
            "error": "tool_call_limit_exceeded",
            "tool_name": self.name,
            "message": format!("{message}. Do not call this tool again; continue with the available information or choose another allowed action."),
            "max_calls": self.max_calls,
        })
    }
}

impl ModelRequest {
    fn call_limit_error(&self, tool_definition: &ModelToolDefinition) -> Option<Value> {
        let message = self
            .tool_call_tracker
            .register_call(&tool_definition.name, tool_definition.max_calls, &tool_definition.max_calls_scope)
            .err()?;
        let tool_error = tool_definition.call_limit_error(message);

        self.send_tool_call_failed(tool_definition.name.clone(), tool_error.clone(), Duration::ZERO);
        log::warn!(
            "rejected tool call at max_calls limit: agent={}, tool={}, error={}",
            self.agent_name,
            tool_definition.name,
            tool_error.get("message").and_then(Value::as_str).unwrap_or("max_calls exceeded")
        );

        Some(tool_error)
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

    fn send_tool_call_failed(&self, tool_name: String, error: Value, duration: Duration);

    fn send_tool_call_completed(&self, tool_name: String, result: Value, duration: Duration);

    fn send_mcp_tool_validation_started(&self, tool_name: String, arguments: Value, input_schema: Value);

    fn send_mcp_tool_validation_failed(&self, tool_name: String, error: Value, duration: Duration);

    fn send_mcp_tool_validation_completed(&self, tool_name: String, duration: Duration);

    fn send_mcp_call_started(&self, details: McpCallEventDetails);

    fn send_mcp_call_failed(&self, details: McpCallEventDetails, error: Value, duration: Duration);

    fn send_mcp_call_completed(&self, details: McpCallEventDetails, result: Value, raw_result: Value, duration: Duration);
}

impl ToolCallEventSender for ModelRequest {
    fn send_tool_call_started(&self, tool_name: String, arguments: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_started(self.agent_name.clone(), tool_name, arguments));
        }
    }

    fn send_tool_call_failed(&self, tool_name: String, error: Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_failed(self.agent_name.clone(), tool_name, error, duration));
        }
    }

    fn send_tool_call_completed(&self, tool_name: String, result: Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_completed(
                self.agent_name.clone(),
                tool_name,
                result,
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_started(&self, tool_name: String, arguments: Value, input_schema: Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_started(
                self.agent_name.clone(),
                tool_name,
                arguments,
                input_schema,
            ));
        }
    }

    fn send_mcp_tool_validation_failed(&self, tool_name: String, error: Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_failed(
                self.agent_name.clone(),
                tool_name,
                error,
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_completed(&self, tool_name: String, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_completed(
                self.agent_name.clone(),
                tool_name,
                duration,
            ));
        }
    }

    fn send_mcp_call_started(&self, details: McpCallEventDetails) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_call_started(details).with_agent_name(self.agent_name.clone()));
        }
    }

    fn send_mcp_call_failed(&self, details: McpCallEventDetails, error: Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ =
                event_sender.try_send(ExecutorEvent::mcp_call_failed(details, error, duration).with_agent_name(self.agent_name.clone()));
        }
    }

    fn send_mcp_call_completed(&self, details: McpCallEventDetails, result: Value, raw_result: Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(
                ExecutorEvent::mcp_call_completed(details, result, raw_result, duration).with_agent_name(self.agent_name.clone()),
            );
        }
    }
}
