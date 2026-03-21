use crate::ast::Agent;
use crate::providers::error::ProviderError;
use crate::providers::provider::{AgentOutput, Message, Provider, ToolCall, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AnthropicProvider {
    name: String,
    models: Vec<String>,
    api_key: String,
    endpoint: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContent {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContent>,
    model: String,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl AnthropicProvider {
    #[must_use]
    pub fn new(name: String, api_key: String, models: Vec<String>) -> Self {
        Self::with_endpoint(name, api_key, "https://api.anthropic.com/v1/messages".to_string(), models)
    }

    #[must_use]
    pub fn with_endpoint(name: String, api_key: String, endpoint: String, models: Vec<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            name,
            models,
            api_key,
            endpoint,
            client,
        }
    }

    fn parse_sse_response(response_text: &str) -> Result<AnthropicResponse, ProviderError> {
        let mut final_message: Option<Value> = None;
        let mut content_blocks: Vec<Value> = Vec::new();
        let mut current_tool_use: Option<Value> = None;

        for line in response_text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<Value>(data) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("message_start") => {
                            if let Some(message) = event.get("message") {
                                final_message = Some(message.clone());
                            }
                        }
                        Some("content_block_start") => {
                            if let Some(block) = event.get("content_block") {
                                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                    current_tool_use = Some(block.clone());
                                }
                            }
                        }
                        Some("content_block_delta") => {
                            if let Some(delta) = event.get("delta") {
                                if let Some(partial_json) = delta.get("partial_json").and_then(|p| p.as_str()) {
                                    if let Some(ref mut tool_use) = current_tool_use {
                                        let existing_input = tool_use
                                            .get("input")
                                            .and_then(|i| i.as_object())
                                            .map_or_else(|| "{}".to_string(), |o| serde_json::to_string(o).unwrap_or_default());

                                        let combined = if existing_input == "{}" {
                                            partial_json.to_string()
                                        } else {
                                            existing_input.trim_end_matches('}').to_string() + "," + partial_json.trim_start_matches('{')
                                        };

                                        if let Ok(parsed_input) = serde_json::from_str::<Value>(&combined) {
                                            if let Some(obj) = tool_use.as_object_mut() {
                                                obj.insert("input".to_string(), parsed_input);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some("content_block_stop") => {
                            if let Some(tool_use) = current_tool_use.take() {
                                content_blocks.push(tool_use);
                            }
                        }
                        Some("message_delta") => {
                            if let Some(delta) = event.get("delta") {
                                if let Some(stop_reason) = delta.get("stop_reason") {
                                    if let Some(ref mut msg) = final_message {
                                        if let Some(obj) = msg.as_object_mut() {
                                            obj.insert("stop_reason".to_string(), stop_reason.clone());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(mut message) = final_message {
            if let Some(obj) = message.as_object_mut() {
                obj.insert("content".to_string(), Value::Array(content_blocks));
            }

            serde_json::from_value(message).map_err(|error| ProviderError::InvalidInput {
                message: format!("Failed to parse reconstructed message: {error}"),
            })
        } else {
            Err(ProviderError::InvalidInput {
                message: "No message found in SSE stream".to_string(),
            })
        }
    }

    fn convert_messages_to_anthropic_format(messages: &[Message]) -> Result<(Option<String>, Vec<AnthropicMessage>), ProviderError> {
        let mut system_prompt = None;
        let mut anthropic_messages = Vec::new();

        for message in messages {
            match message {
                Message::System { content } => {
                    system_prompt = Some(content.clone());
                }
                Message::User { content } => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContent::Text { text: content.clone() }],
                    });
                }
                Message::Assistant { content, tool_calls } => {
                    let mut content_blocks = Vec::new();

                    if !content.is_empty() {
                        content_blocks.push(AnthropicContent::Text { text: content.clone() });
                    }

                    if let Some(calls) = tool_calls {
                        for call in calls {
                            let input: Value = serde_json::from_str(&call.arguments).map_err(|error| ProviderError::InvalidInput {
                                message: format!("Failed to parse tool call arguments: {error}"),
                            })?;

                            content_blocks.push(AnthropicContent::ToolUse {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                input,
                            });
                        }
                    }

                    anthropic_messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: content_blocks,
                    });
                }
                Message::Tool { tool_call_id, content } => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContent::ToolResult {
                            tool_use_id: tool_call_id.clone(),
                            content: content.clone(),
                        }],
                    });
                }
            }
        }

        Ok((system_prompt, anthropic_messages))
    }

    fn build_anthropic_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|tool| AnthropicTool {
                name: tool.name.to_string(),
                description: tool.description.to_string(),
                input_schema: tool.parameters_schema.clone(),
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    async fn execute_agent(&self, agent: &Agent, context: Vec<Message>, tools: Vec<ToolDefinition>) -> Result<AgentOutput, ProviderError> {
        log::debug!("AnthropicProvider executing agent: {}", agent.name);

        let (system_prompt, anthropic_messages) = Self::convert_messages_to_anthropic_format(&context)?;
        log::trace!("Converted {} messages to Anthropic format", anthropic_messages.len());

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
            .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());

        log::info!("Using model: {model_name}");

        let anthropic_tools = Self::build_anthropic_tools(&tools);
        log::debug!("Configured {} tools for Anthropic", anthropic_tools.len());

        let request = AnthropicRequest {
            model: model_name.clone(),
            max_tokens: 4096,
            system: system_prompt,
            messages: anthropic_messages,
            tools: if anthropic_tools.is_empty() { None } else { Some(anthropic_tools) },
            stream: false,
        };

        log::debug!("Sending request to Anthropic");

        let max_retries = 3;
        let mut retry_count = 0;
        let mut wait_time = std::time::Duration::from_secs(30);

        let response: AnthropicResponse = loop {
            let response_result = self
                .client
                .post(&self.endpoint)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match response_result {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let response_text = response.text().await.map_err(|error| ProviderError::ApiError {
                            message: format!("Failed to read response body: {error}"),
                            status_code: None,
                            suggestion: Some("Network error while reading response".to_string()),
                        })?;

                        log::trace!("Anthropic response body: {response_text}");

                        let parsed = if response_text.starts_with("event:") || response_text.starts_with("data:") {
                            log::debug!("Detected SSE format response, parsing as stream");
                            Self::parse_sse_response(&response_text)?
                        } else {
                            serde_json::from_str::<AnthropicResponse>(&response_text).map_err(|error| {
                                log::error!("Failed to parse Anthropic response: {error}");
                                log::error!("Response body: {response_text}");
                                ProviderError::ApiError {
                                    message: format!("Failed to parse Anthropic response: {error}"),
                                    status_code: None,
                                    suggestion: Some(format!(
                                        "The API response format may be incompatible. Response: {}",
                                        &response_text[..response_text.len().min(500)]
                                    )),
                                }
                            })?
                        };

                        break Ok(parsed);
                    } else {
                        let status_code = status.as_u16();
                        let error_text = response.text().await.unwrap_or_default();
                        log::error!("Anthropic API error {status_code}: {error_text}");

                        let is_rate_limit = status_code == 429;

                        if is_rate_limit && retry_count < max_retries {
                            retry_count += 1;
                            log::warn!("Rate limit hit. Waiting {wait_time:?} before retry {retry_count}/{max_retries}");
                            tokio::time::sleep(wait_time).await;
                            wait_time *= 2;
                            continue;
                        }

                        let suggestion = match status_code {
                            401 => Some("Invalid API key. Check your config { api_key: \"...\" }".to_string()),
                            404 => Some("Model not found. Check your model name".to_string()),
                            429 => Some("Rate limit exceeded after retries. Wait longer and try again".to_string()),
                            500 => Some("Server error. Try again later".to_string()),
                            _ => Some("Check your API configuration and network connection".to_string()),
                        };

                        break Err(ProviderError::ApiError {
                            message: format!("Anthropic API error {status_code}: {error_text}"),
                            status_code: Some(status_code),
                            suggestion,
                        });
                    }
                }
                Err(error) => {
                    log::error!("Anthropic API request failed: {error}");
                    break Err(ProviderError::ApiError {
                        message: format!("Anthropic API request failed: {error}"),
                        status_code: None,
                        suggestion: Some("Check your network connection".to_string()),
                    });
                }
            }
        }?;

        log::debug!("Received successful response from Anthropic");

        let mut output_content = String::new();
        let mut tool_calls = Vec::new();

        for content_block in &response.content {
            match content_block {
                AnthropicContent::Text { text } => {
                    output_content.push_str(text);
                }
                AnthropicContent::ToolUse { id, name, input } => {
                    let arguments = serde_json::to_string(input).map_err(|error| ProviderError::InvalidInput {
                        message: format!("Failed to serialize tool use input: {error}"),
                    })?;

                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments,
                    });
                }
                AnthropicContent::ToolResult { .. } => {}
            }
        }

        log::debug!("Anthropic response content length: {} chars", output_content.len());

        let tool_calls_option = if tool_calls.is_empty() {
            None
        } else {
            log::debug!("Anthropic response contains {} tool calls", tool_calls.len());
            Some(tool_calls)
        };

        let mut updated_context = Vec::with_capacity(context.len() + 1);
        updated_context.extend_from_slice(&context);
        updated_context.push(Message::Assistant {
            content: output_content.clone(),
            tool_calls: tool_calls_option,
        });

        log::info!("AnthropicProvider execution completed successfully");

        Ok(AgentOutput {
            output: Value::String(output_content),
            context: updated_context,
        })
    }
}

pub struct AnthropicProviderBuilder;

impl crate::providers::builder::ProviderBuilder for AnthropicProviderBuilder {
    fn build(&self, provider: &crate::ast::Provider) -> Result<crate::providers::provider::ProviderRef, ProviderError> {
        log::debug!("Building Anthropic provider with config: {:?}", provider.config);

        let api_key = provider
            .config
            .get("api_key")
            .and_then(|v| match v {
                crate::ast::Value::String(s) | crate::ast::Value::Interpolated(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| ProviderError::InvalidInput {
                message: "API key not found. Provide it in config { api_key: \"...\" }".to_string(),
            })?;

        let endpoint = provider.config.get("endpoint").and_then(|v| match v {
            crate::ast::Value::String(s) | crate::ast::Value::Interpolated(s) => Some(s.clone()),
            _ => None,
        });

        let anthropic_provider = if let Some(endpoint_url) = endpoint {
            AnthropicProvider::with_endpoint(provider.name.clone(), api_key, endpoint_url, provider.models.clone())
        } else {
            AnthropicProvider::new(provider.name.clone(), api_key, provider.models.clone())
        };

        Ok(std::sync::Arc::new(anthropic_provider))
    }

    fn driver_name(&self) -> &'static str {
        "anthropic"
    }
}
