use crate::context::Context;
use crate::error::ProviderError;
use crate::message::{Message, ToolCall};
use crate::traits::{Provider, ProviderResponse, StopReason, TokenUsage, ToolDefinition};
use crate::AgentConfig;
use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{json, Value};

struct HttpResponseData {
    status_code: StatusCode,
    response_body: String,
    retry_after_seconds: Option<u64>,
}

const DEFAULT_OPENAI_API_BASE: &str = "https://api.openai.com/v1";

pub struct OpenAIProvider {
    http_client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new_with_base_url(DEFAULT_OPENAI_API_BASE, api_key, model)
    }

    pub fn new_with_base_url(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let normalized_base_url = base_url.into().trim_end_matches('/').to_string();

        Self {
            http_client: reqwest::Client::new(),
            base_url: normalized_base_url,
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    pub fn new_local(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new_with_base_url(base_url, String::new(), model)
    }

    fn build_endpoint_url(&self, endpoint_path: &str) -> String {
        format!("{}{}", self.base_url, endpoint_path)
    }

    fn convert_message_to_chat_json(&self, message: &Message) -> Result<Value, String> {
        match message {
            Message::User { content } => Ok(json!({
                "role": "user",
                "content": content,
            })),
            Message::Assistant { content } => Ok(json!({
                "role": "assistant",
                "content": content,
            })),
            Message::AssistantToolCall { tool: tool_call } => {
                let mut assistant_message = json!({
                    "role": "assistant",
                    "content": "",
                });

                assistant_message["tool_calls"] = json!([
                    {
                        "id": tool_call.id,
                        "type": "function",
                        "function": {
                            "name": tool_call.name,
                            "arguments": tool_call.arguments.to_string(),
                        }
                    }
                ]);

                Ok(assistant_message)
            }
            Message::System { content } => Ok(json!({
                "role": "system",
                "content": content,
            })),
            Message::ToolResult { result: tool_result } => {
                let content = serde_json::to_string(tool_result.content()).unwrap_or_else(|_| tool_result.content().to_string());

                Ok(json!({
                    "role": "tool",
                    "tool_call_id": tool_result.tool_call_id(),
                    "content": content,
                }))
            }
        }
    }

    fn convert_tools_to_chat_json(&self, tools: &[ToolDefinition]) -> Result<Vec<Value>, String> {
        let mut converted_tools = Vec::new();

        for tool in tools {
            let parameters = serde_json::to_value(&tool.parameters_schema)
                .map_err(|error| format!("Failed to serialize schema for '{}': {error}", tool.name))?;

            converted_tools.push(json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": parameters,
                }
            }));
        }

        Ok(converted_tools)
    }

    fn convert_message_to_responses_items(&self, message: &Message) -> Result<Vec<Value>, String> {
        let mut response_items = Vec::new();

        match message {
            Message::User { content } => {
                response_items.push(json!({
                    "type": "message",
                    "role": "user",
                    "content": content,
                }));
            }
            Message::Assistant { content } => {
                response_items.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": content,
                }));
            }
            Message::AssistantToolCall { tool: tool_call } => {
                let function_call_item_id = if tool_call.id.starts_with("fc") {
                    tool_call.id.clone()
                } else {
                    format!("fc_{}", tool_call.id)
                };

                response_items.push(json!({
                    "type": "function_call",
                    "id": function_call_item_id,
                    "call_id": tool_call.id,
                    "name": tool_call.name,
                    "arguments": tool_call.arguments.to_string(),
                    "status": "completed",
                }));
            }
            Message::System { content } => {
                response_items.push(json!({
                    "type": "message",
                    "role": "system",
                    "content": content,
                }));
            }
            Message::ToolResult { result: tool_result } => {
                let content = serde_json::to_string(tool_result.content()).unwrap_or_else(|_| tool_result.content().to_string());

                response_items.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_result.tool_call_id(),
                    "output": content,
                }));
            }
        }

        Ok(response_items)
    }

    fn convert_tools_to_responses_json(&self, tools: &[ToolDefinition]) -> Result<Vec<Value>, String> {
        let mut converted_tools = Vec::new();

        for tool in tools {
            let parameters = serde_json::to_value(&tool.parameters_schema)
                .map_err(|error| format!("Failed to serialize schema for '{}': {error}", tool.name))?;

            converted_tools.push(json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": parameters,
                "strict": false,
            }));
        }

        Ok(converted_tools)
    }

    async fn send_request(&self, endpoint_path: &str, request_body: &Value) -> Result<HttpResponseData, ProviderError> {
        let endpoint_url = self.build_endpoint_url(endpoint_path);

        let mut request_builder = self
            .http_client
            .post(endpoint_url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(request_body);

        if !self.api_key.is_empty() {
            request_builder = request_builder.bearer_auth(&self.api_key);
        }

        let response = request_builder.send().await.map_err(|error| ProviderError::Network {
            message: format!("Failed to send request to '{endpoint_path}': {error}"),
        })?;

        let status_code = response.status();
        let retry_after_seconds = response
            .headers()
            .get("retry-after")
            .and_then(|header_value| header_value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());

        let response_body = response.text().await.map_err(|error| ProviderError::Network {
            message: format!("Failed reading response body from '{endpoint_path}': {error}"),
        })?;

        Ok(HttpResponseData {
            status_code,
            response_body,
            retry_after_seconds,
        })
    }

    fn map_http_error(
        &self,
        endpoint_path: &str,
        status_code: StatusCode,
        response_body: String,
        retry_after_seconds: Option<u64>,
    ) -> ProviderError {
        let message = format!("endpoint={endpoint_path} status={} body={response_body}", status_code.as_u16(),);

        match status_code {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::AuthenticationFailed { message },
            StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited {
                message,
                retry_after_seconds,
            },
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => ProviderError::InvalidRequest { message },
            StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_EARLY
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT => ProviderError::ServiceUnavailable { message },
            _ => {
                if status_code.is_server_error() {
                    ProviderError::ServiceUnavailable { message }
                } else {
                    ProviderError::Other { message }
                }
            }
        }
    }

    fn parse_chat_content(content: Option<&Value>) -> Option<String> {
        let content_value = content?;

        if let Some(text_content) = content_value.as_str() {
            return Some(text_content.to_string());
        }

        let content_items = content_value.as_array()?;

        let mut text_parts = Vec::new();

        for content_item in content_items {
            if let Some(text_value) = content_item.get("text").and_then(Value::as_str) {
                text_parts.push(text_value.to_string());
            }
        }

        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        }
    }

    fn parse_tool_call_arguments(arguments: Option<&Value>) -> Value {
        let Some(arguments_value) = arguments else {
            return json!({});
        };

        if arguments_value.is_object() {
            return arguments_value.clone();
        }

        let Some(arguments_string) = arguments_value.as_str() else {
            return json!({});
        };

        serde_json::from_str(arguments_string).unwrap_or_else(|_| json!({ "raw_arguments": arguments_string }))
    }

    fn convert_chat_finish_reason_to_stop_reason(finish_reason: Option<&str>) -> StopReason {
        match finish_reason {
            Some("stop") => StopReason::EndOfSequence,
            Some("length") => StopReason::MaxTokens,
            Some("tool_calls") => StopReason::ToolCalls,
            Some("content_filter") => StopReason::ContentFilter,
            Some("function_call") => StopReason::ToolCalls,
            Some(other_reason) => StopReason::Other(format!("Unhandled finish_reason: {other_reason}")),
            None => StopReason::Other("No finish reason provided".to_string()),
        }
    }

    fn parse_chat_response(&self, response_body: &str) -> Result<ProviderResponse, ProviderError> {
        let response_json: Value = serde_json::from_str(response_body).map_err(|error| ProviderError::ResponseParseFailed {
            message: format!("Failed to parse /chat/completions response JSON: {error}. Body: {response_body}"),
        })?;

        let choices = response_json
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::ResponseParseFailed {
                message: format!("Missing or invalid 'choices' in /chat/completions response. Body: {response_body}"),
            })?;

        let first_choice = choices.first().ok_or_else(|| ProviderError::ResponseParseFailed {
            message: format!("No choices in /chat/completions response. Body: {response_body}"),
        })?;

        let message = first_choice.get("message").ok_or_else(|| ProviderError::ResponseParseFailed {
            message: format!("Missing 'message' in first choice. Body: {response_body}"),
        })?;

        let text = Self::parse_chat_content(message.get("content"));

        let finish_reason = first_choice.get("finish_reason").and_then(Value::as_str);
        let stop_reason = Self::convert_chat_finish_reason_to_stop_reason(finish_reason);

        let mut tool_calls = Vec::new();

        if let Some(tool_call_items) = message.get("tool_calls").and_then(Value::as_array) {
            for tool_call_item in tool_call_items {
                let tool_call_id = tool_call_item
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ProviderError::ResponseParseFailed {
                        message: format!("Tool call missing 'id' in /chat/completions response. Body: {response_body}"),
                    })?
                    .to_string();

                let function = tool_call_item.get("function").ok_or_else(|| ProviderError::ResponseParseFailed {
                    message: format!("Tool call missing 'function' in /chat/completions response. Body: {response_body}"),
                })?;

                let function_name = function
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ProviderError::ResponseParseFailed {
                        message: format!("Tool call function missing 'name' in /chat/completions response. Body: {response_body}"),
                    })?
                    .to_string();

                let function_arguments = Self::parse_tool_call_arguments(function.get("arguments"));

                tool_calls.push(ToolCall {
                    id: tool_call_id,
                    name: function_name,
                    arguments: function_arguments,
                });
            }
        }

        let usage = Self::parse_chat_usage(&response_json);

        Ok(ProviderResponse {
            tool_calls,
            text,
            stop_reason,
            usage,
        })
    }

    fn parse_chat_usage(response_json: &Value) -> Option<TokenUsage> {
        let usage = response_json.get("usage")?;

        let total_tokens = usage.get("total_tokens").and_then(Value::as_u64).unwrap_or(0) as usize;
        let input_tokens = usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as usize;
        let output_tokens = usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0) as usize;

        if total_tokens == 0 && input_tokens == 0 && output_tokens == 0 {
            None
        } else {
            Some(TokenUsage {
                total_tokens,
                input_tokens,
                output_tokens,
            })
        }
    }

    fn convert_responses_status_to_stop_reason(status: Option<&str>, has_tool_calls: bool) -> StopReason {
        if has_tool_calls {
            return StopReason::ToolCalls;
        }

        match status {
            Some("completed") => StopReason::EndOfSequence,
            Some("incomplete") => StopReason::MaxTokens,
            Some("failed") => StopReason::Other("Response status is failed".to_string()),
            Some("in_progress") => StopReason::Other("Response status is in_progress".to_string()),
            Some(other_status) => StopReason::Other(format!("Unhandled response status: {other_status}")),
            None => StopReason::Other("Missing response status".to_string()),
        }
    }

    fn parse_responses_message_text(output_item: &Value, text_parts: &mut Vec<String>) {
        let Some(content_items) = output_item.get("content").and_then(Value::as_array) else {
            return;
        };

        for content_item in content_items {
            if let Some(text_value) = content_item.get("text").and_then(Value::as_str) {
                text_parts.push(text_value.to_string());
                continue;
            }

            if let Some(text_value) = content_item.get("output_text").and_then(Value::as_str) {
                text_parts.push(text_value.to_string());
            }
        }
    }

    fn parse_responses_response(&self, response_body: &str) -> Result<ProviderResponse, ProviderError> {
        let response_json: Value = serde_json::from_str(response_body).map_err(|error| ProviderError::ResponseParseFailed {
            message: format!("Failed to parse /responses response JSON: {error}. Body: {response_body}"),
        })?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        if let Some(top_level_output_text) = response_json.get("output_text").and_then(Value::as_str) {
            text_parts.push(top_level_output_text.to_string());
        }

        if let Some(output_items) = response_json.get("output").and_then(Value::as_array) {
            for output_item in output_items {
                let output_item_type = output_item.get("type").and_then(Value::as_str).unwrap_or("");

                if output_item_type == "function_call" {
                    let tool_call_id = output_item
                        .get("call_id")
                        .or_else(|| output_item.get("id"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| ProviderError::ResponseParseFailed {
                            message: format!("Function call missing id/call_id in /responses output. Body: {response_body}"),
                        })?
                        .to_string();

                    let function_name = output_item
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ProviderError::ResponseParseFailed {
                            message: format!("Function call missing name in /responses output. Body: {response_body}"),
                        })?
                        .to_string();

                    let function_arguments = Self::parse_tool_call_arguments(output_item.get("arguments"));

                    tool_calls.push(ToolCall {
                        id: tool_call_id,
                        name: function_name,
                        arguments: function_arguments,
                    });

                    continue;
                }

                if output_item_type == "message" {
                    Self::parse_responses_message_text(output_item, &mut text_parts);
                }
            }
        }

        let text = if text_parts.is_empty() { None } else { Some(text_parts.join("\n")) };

        let status = response_json.get("status").and_then(Value::as_str);
        let stop_reason = Self::convert_responses_status_to_stop_reason(status, !tool_calls.is_empty());
        let usage = Self::parse_responses_usage(&response_json);

        Ok(ProviderResponse {
            tool_calls,
            text,
            stop_reason,
            usage,
        })
    }

    fn parse_responses_usage(response_json: &Value) -> Option<TokenUsage> {
        let usage = response_json.get("usage")?;

        let input_tokens = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) as usize;
        let output_tokens = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0) as usize;
        let total_tokens = usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .map(|tokens| tokens as usize)
            .unwrap_or(input_tokens + output_tokens);

        if total_tokens == 0 && input_tokens == 0 && output_tokens == 0 {
            None
        } else {
            Some(TokenUsage {
                total_tokens,
                input_tokens,
                output_tokens,
            })
        }
    }

    fn parse_tool_call_from_responses_item(&self, item: &Value) -> Result<Option<ToolCall>, ProviderError> {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

        if item_type != "function_call" {
            return Ok(None);
        }

        let tool_call_id = item
            .get("call_id")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::ResponseParseFailed {
                message: format!("Function call item missing id/call_id in /responses stream item: {item}"),
            })?
            .to_string();

        let function_name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::ResponseParseFailed {
                message: format!("Function call item missing name in /responses stream item: {item}"),
            })?
            .to_string();

        let function_arguments = Self::parse_tool_call_arguments(item.get("arguments"));

        Ok(Some(ToolCall {
            id: tool_call_id,
            name: function_name,
            arguments: function_arguments,
        }))
    }

    fn parse_responses_sse_response(&self, response_body: &str) -> Result<ProviderResponse, ProviderError> {
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut completed_response_payload: Option<Value> = None;

        for response_line in response_body.lines() {
            let trimmed_line = response_line.trim();

            if !trimmed_line.starts_with("data:") {
                continue;
            }

            let data_payload = trimmed_line.trim_start_matches("data:").trim();

            if data_payload.is_empty() || data_payload == "[DONE]" {
                continue;
            }

            let event_json: Value = match serde_json::from_str(data_payload) {
                Ok(parsed_event) => parsed_event,
                Err(_) => continue,
            };

            let event_type = event_json.get("type").and_then(Value::as_str).unwrap_or("");

            if event_type == "response.completed" {
                if let Some(response_payload) = event_json.get("response") {
                    completed_response_payload = Some(response_payload.clone());
                }

                continue;
            }

            if event_type == "response.failed" {
                return Err(ProviderError::Other {
                    message: format!("OpenAI responses stream failure event: {event_json}"),
                });
            }

            if event_type == "response.output_text.delta" {
                if let Some(delta_text) = event_json.get("delta").and_then(Value::as_str) {
                    text_parts.push(delta_text.to_string());
                }

                continue;
            }

            if event_type == "response.output_item.done" {
                if let Some(item) = event_json.get("item") {
                    if let Some(tool_call) = self.parse_tool_call_from_responses_item(item)? {
                        tool_calls.push(tool_call);
                    }

                    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

                    if item_type == "message" {
                        Self::parse_responses_message_text(item, &mut text_parts);
                    }
                }

                continue;
            }

            if event_type == "response.function_call_arguments.done" {
                if let Some(tool_call) = self.parse_tool_call_from_responses_item(&event_json)? {
                    tool_calls.push(tool_call);
                }
            }
        }

        if let Some(completed_response) = completed_response_payload {
            return self.parse_responses_response(&completed_response.to_string());
        }

        let text = if text_parts.is_empty() { None } else { Some(text_parts.join("")) };

        if text.is_some() || !tool_calls.is_empty() {
            let stop_reason = if tool_calls.is_empty() {
                StopReason::EndOfSequence
            } else {
                StopReason::ToolCalls
            };

            return Ok(ProviderResponse {
                tool_calls,
                text,
                stop_reason,
                usage: None,
            });
        }

        Err(ProviderError::ResponseParseFailed {
            message: format!("Failed to parse streamed /responses payload. Body did not contain parsable data events: {response_body}"),
        })
    }

    fn parse_responses_response_or_stream(&self, response_body: &str) -> Result<ProviderResponse, ProviderError> {
        if response_body.trim_start().starts_with('{') {
            return self.parse_responses_response(response_body);
        }

        self.parse_responses_sse_response(response_body)
    }

    fn build_chat_request_body(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<Value, String> {
        let messages: Result<Vec<Value>, String> = context
            .messages
            .iter()
            .map(|message| self.convert_message_to_chat_json(message))
            .collect();

        let mut request_body = json!({
            "model": self.model,
            "messages": messages?,
            "parallel_tool_calls": true,
        });

        if let Some(temperature) = config.temperature {
            request_body["temperature"] = json!(temperature);
        }

        if let Some(top_p) = config.top_p {
            request_body["top_p"] = json!(top_p);
        }

        if let Some(max_tokens) = config.max_tokens {
            request_body["max_tokens"] = json!(max_tokens);
        }

        if let Some(frequency_penalty) = config.frequency_penalty {
            request_body["frequency_penalty"] = json!(frequency_penalty);
        }

        if let Some(presence_penalty) = config.presence_penalty {
            request_body["presence_penalty"] = json!(presence_penalty);
        }

        if let Some(seed) = config.seed {
            request_body["seed"] = json!(seed);
        }

        if let Some(stop_sequences) = &config.stop_sequences {
            request_body["stop"] = json!(stop_sequences);
        }

        if !tools.is_empty() {
            request_body["tools"] = json!(self.convert_tools_to_chat_json(tools)?);
        }

        Ok(request_body)
    }

    fn build_responses_request_body(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<Value, String> {
        let mut input_items = Vec::new();

        for message in &context.messages {
            let mut items_for_message = self.convert_message_to_responses_items(message)?;
            input_items.append(&mut items_for_message);
        }

        let mut request_body = json!({
            "model": self.model,
            "input": input_items,
            "parallel_tool_calls": true,
            "store": false,
            "stream": true,
        });

        if let Some(temperature) = config.temperature {
            request_body["temperature"] = json!(temperature);
        }

        if let Some(top_p) = config.top_p {
            request_body["top_p"] = json!(top_p);
        }

        if let Some(max_tokens) = config.max_tokens {
            request_body["max_output_tokens"] = json!(max_tokens);
        }

        if let Some(frequency_penalty) = config.frequency_penalty {
            request_body["frequency_penalty"] = json!(frequency_penalty);
        }

        if let Some(presence_penalty) = config.presence_penalty {
            request_body["presence_penalty"] = json!(presence_penalty);
        }

        if let Some(seed) = config.seed {
            request_body["seed"] = json!(seed);
        }

        if let Some(stop_sequences) = &config.stop_sequences {
            request_body["stop"] = json!(stop_sequences);
        }

        if !tools.is_empty() {
            request_body["tools"] = json!(self.convert_tools_to_responses_json(tools)?);
        }

        Ok(request_body)
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<ProviderResponse, ProviderError> {
        let chat_request_body = self
            .build_chat_request_body(context, tools, config)
            .map_err(|message| ProviderError::InvalidRequest { message })?;
        let chat_response = self.send_request("/chat/completions", &chat_request_body).await?;

        if chat_response.status_code.is_success() {
            return self.parse_chat_response(&chat_response.response_body);
        }

        if chat_response.status_code != StatusCode::NOT_FOUND {
            return Err(self.map_http_error(
                "/chat/completions",
                chat_response.status_code,
                chat_response.response_body,
                chat_response.retry_after_seconds,
            ));
        }

        let responses_request_body = self
            .build_responses_request_body(context, tools, config)
            .map_err(|message| ProviderError::InvalidRequest { message })?;
        let responses_response = self.send_request("/responses", &responses_request_body).await?;

        if responses_response.status_code.is_success() {
            return self.parse_responses_response_or_stream(&responses_response.response_body);
        }

        Err(self.map_http_error(
            "/responses",
            responses_response.status_code,
            responses_response.response_body,
            responses_response.retry_after_seconds,
        ))
    }
}
