use crate::ast::Agent;
use crate::providers::error::ProviderError;
use crate::providers::provider::{AgentOutput, Message, Provider, ToolCall, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

pub struct OllamaProvider {
    name: String,
    api_endpoint: String,
    models: Vec<String>,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaMessage {
    role: Cow<'static, str>,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    function: OllamaToolFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolFunction {
    name: String,
    arguments: Value,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaToolFunctionDef,
}

#[derive(Debug, Serialize)]
struct OllamaToolFunctionDef {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

impl OllamaProvider {
    #[must_use]
    pub fn new(name: String, api_endpoint: String, models: Vec<String>) -> Self {
        let client = reqwest::Client::new();

        Self {
            name,
            api_endpoint,
            models,
            client,
        }
    }

    fn convert_messages_to_ollama_format(messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|message| match message {
                Message::System { content } => OllamaMessage {
                    role: Cow::Borrowed("system"),
                    content: content.clone(),
                    tool_calls: None,
                },
                Message::User { content } => OllamaMessage {
                    role: Cow::Borrowed("user"),
                    content: content.clone(),
                    tool_calls: None,
                },
                Message::Assistant { content, tool_calls } => {
                    let ollama_tool_calls = tool_calls.as_ref().map(|calls| {
                        calls
                            .iter()
                            .map(|call| {
                                let arguments: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);

                                OllamaToolCall {
                                    function: OllamaToolFunction {
                                        name: call.name.clone(),
                                        arguments,
                                    },
                                }
                            })
                            .collect()
                    });

                    OllamaMessage {
                        role: Cow::Borrowed("assistant"),
                        content: content.clone(),
                        tool_calls: ollama_tool_calls,
                    }
                }
                Message::Tool { tool_call_id: _, content } => OllamaMessage {
                    role: Cow::Borrowed("tool"),
                    content: content.clone(),
                    tool_calls: None,
                },
            })
            .collect()
    }

    fn build_ollama_tools(tools: &[ToolDefinition]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|tool| {
                log::trace!(
                    "Original schema for tool '{}': {}",
                    tool.name,
                    serde_json::to_string_pretty(&tool.parameters_schema).unwrap_or_default()
                );

                let simplified_schema = Self::simplify_schema_for_ollama(&tool.parameters_schema);

                log::debug!(
                    "Building Ollama tool '{}' with schema: {}",
                    tool.name,
                    serde_json::to_string_pretty(&simplified_schema).unwrap_or_default()
                );

                OllamaTool {
                    tool_type: "function".to_string(),
                    function: OllamaToolFunctionDef {
                        name: tool.name.to_string(),
                        description: tool.description.to_string(),
                        parameters: simplified_schema,
                    },
                }
            })
            .collect()
    }

    fn simplify_schema_for_ollama(schema: &Value) -> Value {
        if let Some(obj) = schema.as_object() {
            let mut simplified = serde_json::Map::new();

            simplified.insert("type".to_string(), Value::String("object".to_string()));

            if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
                let mut simple_props = serde_json::Map::new();

                for (key, value) in properties {
                    simple_props.insert(key.clone(), Self::simplify_property(value, obj));
                }

                simplified.insert("properties".to_string(), Value::Object(simple_props));
            }

            if let Some(required) = obj.get("required") {
                simplified.insert("required".to_string(), required.clone());
            }

            Value::Object(simplified)
        } else {
            schema.clone()
        }
    }

    fn simplify_property(prop: &Value, root_schema: &serde_json::Map<String, Value>) -> Value {
        if let Some(obj) = prop.as_object() {
            if let Some(ref_path) = obj.get("$ref").and_then(|v| v.as_str()) {
                if let Some(resolved) = Self::resolve_ref(ref_path, root_schema) {
                    return Self::simplify_property(&resolved, root_schema);
                }
            }

            let mut simple_prop = serde_json::Map::new();

            if let Some(prop_type) = obj.get("type") {
                simple_prop.insert("type".to_string(), prop_type.clone());
            } else if let Some(any_of) = obj.get("anyOf") {
                if let Some(array) = any_of.as_array() {
                    for item in array {
                        if let Some(item_type) = item.get("type") {
                            if item_type.as_str() != Some("null") {
                                simple_prop.insert("type".to_string(), item_type.clone());
                                break;
                            }
                        }
                    }
                }
            } else if let Some(one_of) = obj.get("oneOf") {
                if let Some(array) = one_of.as_array() {
                    if let Some(first) = array.first() {
                        if let Some(first_type) = first.get("type") {
                            simple_prop.insert("type".to_string(), first_type.clone());
                        }
                    }
                }
            }

            if simple_prop.is_empty() {
                simple_prop.insert("type".to_string(), Value::String("string".to_string()));
            }

            if let Some(description) = obj.get("description") {
                simple_prop.insert("description".to_string(), description.clone());
            }

            if let Some(enum_values) = obj.get("enum") {
                simple_prop.insert("enum".to_string(), enum_values.clone());
            }

            if let Some(items) = obj.get("items") {
                simple_prop.insert("items".to_string(), Self::simplify_property(items, root_schema));
            }

            if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
                let mut simple_nested_props = serde_json::Map::new();

                for (key, value) in properties {
                    simple_nested_props.insert(key.clone(), Self::simplify_property(value, root_schema));
                }

                simple_prop.insert("properties".to_string(), Value::Object(simple_nested_props));
            }

            Value::Object(simple_prop)
        } else {
            serde_json::json!({"type": "string"})
        }
    }

    fn resolve_ref(ref_path: &str, root_schema: &serde_json::Map<String, Value>) -> Option<Value> {
        if !ref_path.starts_with("#/") {
            return None;
        }

        let path_parts: Vec<&str> = ref_path[2..].split('/').collect();
        let mut current = Value::Object(root_schema.clone());

        for part in path_parts {
            current = current.get(part)?.clone();
        }

        Some(current)
    }
}

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    #[allow(clippy::too_many_lines)]
    async fn execute_agent(&self, agent: &Agent, context: Vec<Message>, tools: Vec<ToolDefinition>) -> Result<AgentOutput, ProviderError> {
        log::debug!("OllamaProvider executing agent: {}", agent.name);
        log::debug!("Context has {} messages", context.len());

        let ollama_messages = Self::convert_messages_to_ollama_format(&context);
        log::trace!("Converted {} messages to Ollama format", ollama_messages.len());

        for (i, msg) in ollama_messages.iter().enumerate() {
            log::trace!(
                "Message {}: role={}, content_len={}, has_tool_calls={}",
                i,
                msg.role,
                msg.content.len(),
                msg.tool_calls.is_some()
            );
        }

        let model_name = agent
            .properties
            .iter()
            .find_map(|prop| match prop {
                crate::ast::AgentProperty::Model { value, .. } => {
                    let model_ref = match value {
                        crate::ast::Value::String(s) | crate::ast::Value::Interpolated(s) => Some(s.as_str()),
                        _ => None,
                    };

                    model_ref.and_then(|model_ref| model_ref.split('/').nth(1).map(std::string::ToString::to_string))
                }
                _ => None,
            })
            .unwrap_or_else(|| "qwen3:8b".to_string());

        log::info!("Using model: {model_name}");

        let ollama_tools = Self::build_ollama_tools(&tools);
        log::debug!("Configured {} tools for Ollama", ollama_tools.len());

        let request = OllamaChatRequest {
            model: model_name.clone(),
            messages: ollama_messages.clone(),
            tools: ollama_tools,
            stream: false,
        };

        log::trace!("Ollama request: {:?}", serde_json::to_string_pretty(&request).unwrap_or_default());

        let url = format!("{}/api/chat", self.api_endpoint);
        log::debug!("Sending request to Ollama: {url}");

        let response = self.client.post(&url).json(&request).send().await.map_err(|error| {
            log::error!("Ollama HTTP request failed: {error}");
            ProviderError::ApiError {
                message: format!("Ollama HTTP request failed: {error}"),
                status_code: None,
                suggestion: Some("Check that Ollama server is running and accessible".to_string()),
            }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            log::error!("Ollama API error: {status} - {error_text}");

            return Err(ProviderError::ApiError {
                message: format!("Ollama API error: {status} - {error_text}"),
                status_code: Some(status.as_u16()),
                suggestion: Some("Check Ollama server logs for details".to_string()),
            });
        }

        log::debug!("Received successful response from Ollama");

        let ollama_response: OllamaChatResponse = response.json().await.map_err(|error| {
            log::error!("Failed to parse Ollama response: {error}");
            ProviderError::ApiError {
                message: format!("Failed to parse Ollama response: {error}"),
                status_code: None,
                suggestion: Some("Check Ollama API compatibility".to_string()),
            }
        })?;

        log::trace!("Ollama response: {ollama_response:?}");

        let output_content = ollama_response.message.content.clone();
        log::debug!("Ollama response content length: {} chars", output_content.len());

        let tool_calls = ollama_response.message.tool_calls.as_ref().map(|calls| {
            log::debug!("Ollama response contains {} tool calls", calls.len());
            calls
                .iter()
                .enumerate()
                .map(|(index, call)| {
                    log::trace!("Tool call {}: {}", index, call.function.name);
                    ToolCall {
                        id: format!("call_{index}"),
                        name: call.function.name.clone(),
                        arguments: serde_json::to_string(&call.function.arguments).unwrap_or_default(),
                    }
                })
                .collect()
        });

        let mut updated_context = Vec::with_capacity(context.len() + 2);
        updated_context.extend_from_slice(&context);

        if output_content.is_empty() && tool_calls.is_none() {
            log::warn!("Ollama returned empty response with no content and no tool calls");

            updated_context.push(Message::Assistant {
                content: String::new(),
                tool_calls: None,
            });

            updated_context.push(Message::User {
                content: "You returned an empty response. Please provide a response with either text content or tool calls. Remember to use the available tools to complete your task, and call the done tool when finished.".to_string(),
            });

            log::info!("OllamaProvider added feedback for empty response");

            return Ok(AgentOutput {
                output: Value::String(String::new()),
                context: updated_context,
            });
        }

        updated_context.push(Message::Assistant {
            content: output_content.clone(),
            tool_calls,
        });

        log::info!("OllamaProvider execution completed successfully");

        Ok(AgentOutput {
            output: Value::String(output_content),
            context: updated_context,
        })
    }
}

// Provider builder implementation
pub struct OllamaProviderBuilder;

impl crate::providers::builder::ProviderBuilder for OllamaProviderBuilder {
    fn build(&self, provider: &crate::ast::Provider) -> Result<crate::providers::provider::ProviderRef, ProviderError> {
        // Try to get endpoint from config, fall back to default
        let api_endpoint = provider
            .config
            .get("endpoint")
            .and_then(|v| match v {
                crate::ast::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let ollama_provider = OllamaProvider::new(provider.name.clone(), api_endpoint, provider.models.clone());

        Ok(std::sync::Arc::new(ollama_provider))
    }

    fn driver_name(&self) -> &'static str {
        "ollama"
    }
}
