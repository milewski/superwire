use crate::event::{ExecutorEvent, McpCallEventDetails};
use crate::model::provider::ModelProvider;
use crate::model::response::normalize_mcp_tool_result;
use crate::model::types::{
    FinalizeCallKind, ModelAsset, ModelAssetSource, ModelPromptContent, ModelRequest, ModelResponse, ModelSchemaCache, ModelToolDefinition,
    ModelToolSource,
};
use crate::runtime::ExecutorError;
use async_trait::async_trait;
use cersei_provider::{Anthropic, CompletionRequest, Gemini, OpenAi, Provider};
use cersei_types::{
    CitationsConfig, ContentBlock, DocumentSource, ImageSource, Message, MessageContent, ToolDefinition, ToolResultContent,
};
use jsonschema::ValidationError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};
use superwire_core::mcp::McpServerConfig;
use superwire_core::semantic::support::provider::{ProviderApiFormat, ProviderConfig, ProviderDriver};

const MAX_TOOL_CALL_ROUNDS: usize = 8;
const DEFAULT_MAX_TOKENS: u32 = 16_384;

#[derive(Debug, Clone, Default)]
pub struct CerseiModelProvider;

#[async_trait]
impl ModelProvider for CerseiModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let provider = request.provider_config.build_provider(&request)?;
        let mut schema_cache = ModelSchemaCache::new();
        let request_context = request.cersei_request_context(&mut schema_cache)?;
        let mut last_error = None;
        let mut messages = vec![request.cersei_user_message()];

        log::info!(
            "starting Cersei generation: agent={}, provider={}, model={}, tools={}",
            request.agent_name,
            request.provider_config.driver.as_str(),
            request.model_name,
            request.tools.len()
        );

        for round_index in 0..MAX_TOOL_CALL_ROUNDS {
            let completion_request = request_context.build_completion_request(&request.model_name, messages.clone());

            log::debug!(
                "sending provider request through Cersei: agent={}, provider={}, round={}, messages={}, tools={}",
                request.agent_name,
                request.provider_config.driver.as_str(),
                round_index + 1,
                completion_request.messages.len(),
                completion_request.tools.len()
            );

            let completion_result = provider.complete_blocking(completion_request).await;
            let completion = match completion_result {
                Ok(completion) => completion,
                Err(error) => {
                    log::warn!(
                        "provider request failed through Cersei: agent={}, provider={}, round={}, error={}",
                        request.agent_name,
                        request.provider_config.driver.as_str(),
                        round_index + 1,
                        error
                    );
                    last_error = Some(error.to_string());

                    break;
                }
            };

            let tool_calls = CerseiToolCall::from_message(&completion.message);

            if !tool_calls.is_empty() {
                log::info!(
                    "provider requested tool calls: agent={}, provider={}, count={}",
                    request.agent_name,
                    request.provider_config.driver.as_str(),
                    tool_calls.len()
                );
                let tool_call_round = self.execute_tool_calls(&request, &tool_calls, &mut schema_cache)?;

                if let Some(finalize_result) = tool_call_round.finalize_result {
                    return self.complete_generation(&request, finalize_result);
                }

                messages.push(completion.message.without_empty_text_blocks());
                messages.extend(tool_call_round.messages);

                continue;
            }

            let assistant_content = completion.message.non_empty_text();
            messages.push(completion.message);
            messages.push(Message::user(
                "To finish this agent run you must call the internal `finalize` tool. Call `finalize` with ` {\"type\":\"success\",\"output\":...}` when the output is ready and matches the schema, or `{\"type\":\"fail\",\"reason\":\"...\"}` when you cannot fulfill the request. Do not answer with plain text.",
            ));
            last_error = assistant_content
                .map(|content| format!("model stopped with text instead of calling finalize: {content}"))
                .or_else(|| Some("model response did not include finalize tool call".to_string()));
        }

        Err(ExecutorError::Model {
            agent_name: request.agent_name,
            message: last_error.unwrap_or_else(|| "model did not call finalize".to_string()),
        })
    }
}

struct CerseiRequestContext {
    system_prompt: String,
    tool_definitions: Vec<ToolDefinition>,
    max_tokens: u32,
    temperature: Option<f32>,
    options: HashMap<String, Value>,
}

impl CerseiRequestContext {
    fn build_completion_request(&self, model_name: &str, messages: Vec<Message>) -> CompletionRequest {
        let mut completion_request = CompletionRequest::new(model_name.to_string());

        completion_request.system = Some(self.system_prompt.clone());
        completion_request.messages = messages;
        completion_request.tools.clone_from(&self.tool_definitions);
        completion_request.max_tokens = self.max_tokens;
        completion_request.temperature = self.temperature;

        for (setting_name, setting_value) in &self.options {
            completion_request.options.set(setting_name.clone(), setting_value.clone());
        }

        completion_request
    }
}

impl ModelRequest {
    fn cersei_user_message(&self) -> Message {
        if self.prompt_content.is_empty() {
            return Message::user(self.prompt.clone());
        }

        Message::user_blocks(self.cersei_content_blocks())
    }

    fn cersei_content_blocks(&self) -> Vec<ContentBlock> {
        let mut content_blocks = Vec::new();

        for prompt_content in &self.prompt_content {
            match prompt_content {
                ModelPromptContent::Text(text) => {
                    if !text.is_empty() {
                        content_blocks.push(ContentBlock::Text { text: text.clone() });
                    }
                }
                ModelPromptContent::Asset(asset) => {
                    content_blocks.push(asset.cersei_content_block());
                }
            }
        }

        content_blocks
    }
}

impl ModelAsset {
    fn cersei_content_block(&self) -> ContentBlock {
        match self.kind {
            superwire_core::dsl::ModelAssetKind::Image => ContentBlock::Image {
                source: self.cersei_image_source(),
            },
            superwire_core::dsl::ModelAssetKind::Document | superwire_core::dsl::ModelAssetKind::Video => ContentBlock::Document {
                source: self.cersei_document_source(),
                title: self.title.clone(),
                context: self.context.clone(),
                citations: self.citations.map(|enabled| CitationsConfig { enabled }),
            },
        }
    }

    fn cersei_image_source(&self) -> ImageSource {
        let (source_type, data, url) = self.cersei_source_parts();

        ImageSource {
            source_type,
            media_type: self.media_type.clone(),
            data,
            url,
        }
    }

    fn cersei_document_source(&self) -> DocumentSource {
        let (source_type, data, url) = self.cersei_source_parts();

        DocumentSource {
            source_type,
            media_type: self.media_type.clone(),
            data,
            url,
        }
    }

    fn cersei_source_parts(&self) -> (String, Option<String>, Option<String>) {
        match &self.source {
            ModelAssetSource::Url(url) => ("url".to_string(), None, Some(url.clone())),
            ModelAssetSource::Base64(data) => ("base64".to_string(), Some(data.clone()), None),
        }
    }
}

impl CerseiModelProvider {
    fn execute_tool_calls(
        &self,
        request: &ModelRequest,
        tool_calls: &[CerseiToolCall],
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallRound, ExecutorError> {
        let mut messages = Vec::new();

        for tool_call in tool_calls {
            let tool_outcome = self.execute_tool_call(request, tool_call, schema_cache)?;

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

            messages.push(Message::user_blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: ToolResultContent::Text(tool_result_text),
                is_error: tool_result.get("error").is_some().then_some(true),
            }]));
        }

        Ok(ToolCallRound {
            messages,
            finalize_result: None,
        })
    }

    fn execute_tool_call(
        &self,
        request: &ModelRequest,
        tool_call: &CerseiToolCall,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        let tool_definition = request
            .tools
            .iter()
            .find(|tool_definition| tool_definition.name == tool_call.name)
            .ok_or_else(|| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("model requested unknown tool `{}`", tool_call.name),
            })?;
        let tool_call_started_at = Instant::now();

        if let Some(tool_error) = request.call_limit_error(tool_definition) {
            return Ok(ToolCallOutcome::Continue(tool_error));
        }

        log::debug!(
            "processing model tool call: agent={}, requested_tool={}, resolved_tool={}",
            request.agent_name,
            tool_call.name,
            tool_definition.name
        );
        let mut arguments = tool_call.input.clone();
        let validation_started_at = Instant::now();
        let input_schema = tool_definition.input_schema.json_value_with_cache(schema_cache);

        if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
            request.send_mcp_tool_validation_started(&tool_definition.name, &arguments, &input_schema);
        }

        if let Err(message) = validate_tool_arguments(&arguments, &input_schema) {
            let tool_error = tool_definition.argument_error(message, schema_cache);

            if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
                request.send_mcp_tool_validation_failed(&tool_definition.name, &tool_error, validation_started_at.elapsed());
            }

            if !matches!(tool_definition.source, ModelToolSource::Finalize) {
                request.send_tool_call_failed(&tool_definition.name, &tool_error, tool_call_started_at.elapsed());
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
            request.send_mcp_tool_validation_completed(&tool_definition.name, validation_started_at.elapsed());
        }

        if matches!(tool_definition.source, ModelToolSource::Finalize) {
            return tool_definition.parse_finalize_arguments(arguments).map(ToolCallOutcome::Finalized);
        }

        if let (Some(argument_object), Some(binding_object)) = (arguments.as_object_mut(), tool_definition.bindings.as_object()) {
            for (binding_name, binding_value) in binding_object {
                argument_object.insert(binding_name.clone(), binding_value.clone());
            }
        }

        tool_definition.execute_external_tool(request, arguments, schema_cache)
    }

    fn complete_generation(&self, request: &ModelRequest, finalize_result: FinalizeResult) -> Result<ModelResponse, ExecutorError> {
        match finalize_result {
            FinalizeResult::Success(output) => {
                log::info!(
                    "Cersei generation completed: agent={}, provider={}, model={}",
                    request.agent_name,
                    request.provider_config.driver.as_str(),
                    request.model_name
                );

                Ok(ModelResponse {
                    output,
                    context: json!({
                        "provider": request.provider_config.driver.as_str(),
                        "model": request.model_name,
                    }),
                })
            }
            FinalizeResult::Fail(reason) => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("agent finalized with failure: {reason}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferenceParameter {
    MaxTokens,
    Temperature,
}

impl InferenceParameter {
    fn as_str(self) -> &'static str {
        match self {
            Self::MaxTokens => "max_tokens",
            Self::Temperature => "temperature",
        }
    }
}

#[derive(Debug, Clone)]
struct CerseiToolCall {
    id: String,
    name: String,
    input: Value,
}

impl CerseiToolCall {
    fn from_message(message: &Message) -> Vec<Self> {
        message
            .content_blocks()
            .into_iter()
            .filter_map(|content_block| match content_block {
                ContentBlock::ToolUse { id, name, input } => Some(Self { id, name, input }),
                _ => None,
            })
            .collect()
    }
}

struct ToolCallRound {
    messages: Vec<Message>,
    finalize_result: Option<FinalizeResult>,
}

enum ToolCallOutcome {
    Continue(Value),
    Finalized(FinalizeResult),
}

enum FinalizeResult {
    Success(Value),
    Fail(String),
}

struct McpToolTarget<'source> {
    server_name: Option<&'source str>,
    tool_name: &'source str,
    endpoint: &'source str,
    headers: &'source BTreeMap<String, String>,
}

struct McpImportTarget<'source> {
    server_name: &'source str,
    item_name: &'source str,
    endpoint: &'source str,
    headers: &'source BTreeMap<String, String>,
}

impl ModelRequest {
    fn cersei_request_context(&self, schema_cache: &mut ModelSchemaCache) -> Result<CerseiRequestContext, ExecutorError> {
        let output_schema_text = self
            .output_schema
            .json_string_with_cache(schema_cache)
            .map_err(|error| ExecutorError::Model {
                agent_name: self.agent_name.clone(),
                message: format!("failed to serialize output schema: {error}"),
            })?;
        let options = self.cersei_options();

        Ok(CerseiRequestContext {
            system_prompt: format!(
                "You are executing a deterministic workflow agent. You must finish by calling the internal `finalize` tool. Do not end with assistant text. For success, call `finalize` with type `success` and an `output` value that matches this JSON Schema: {output_schema_text}. If you cannot fulfill the request, call `finalize` with type `fail` and a clear `reason`. Never put failure or apology text in a success output."
            ),
            tool_definitions: self
                .tools
                .iter()
                .map(|tool_definition| tool_definition.to_cersei_tool_definition(schema_cache))
                .collect(),
            max_tokens: self.max_tokens(),
            temperature: self.temperature(),
            options,
        })
    }

    fn max_tokens(&self) -> u32 {
        self.inference
            .get(InferenceParameter::MaxTokens.as_str())
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(DEFAULT_MAX_TOKENS)
    }

    fn temperature(&self) -> Option<f32> {
        self.inference
            .get(InferenceParameter::Temperature.as_str())
            .and_then(|value| serde_json::from_value::<f32>(value.clone()).ok())
    }

    fn cersei_options(&self) -> HashMap<String, Value> {
        self.inference
            .iter()
            .filter(|(setting_name, _)| {
                setting_name.as_str() != InferenceParameter::MaxTokens.as_str()
                    && setting_name.as_str() != InferenceParameter::Temperature.as_str()
            })
            .map(|(setting_name, setting_value)| (setting_name.clone(), setting_value.clone()))
            .collect()
    }

    fn call_limit_error(&self, tool_definition: &ModelToolDefinition) -> Option<Value> {
        let message = self
            .tool_call_tracker
            .register_call(&tool_definition.name, tool_definition.max_calls, &tool_definition.max_calls_scope)
            .err()?;
        let tool_error = tool_definition.call_limit_error(message);

        self.send_tool_call_failed(&tool_definition.name, &tool_error, Duration::ZERO);
        log::warn!(
            "rejected tool call at max_calls limit: agent={}, tool={}, error={}",
            self.agent_name,
            tool_definition.name,
            tool_error.get("message").and_then(Value::as_str).unwrap_or("max_calls exceeded")
        );

        Some(tool_error)
    }
}

impl ModelToolDefinition {
    fn to_cersei_tool_definition(&self, schema_cache: &mut ModelSchemaCache) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone().unwrap_or_else(|| format!("Workflow tool `{}`", self.name)),
            input_schema: self.input_schema.json_value_with_cache(schema_cache),
        }
    }

    fn parse_finalize_arguments(&self, arguments: Value) -> Result<FinalizeResult, ExecutorError> {
        match arguments
            .get("type")
            .and_then(Value::as_str)
            .and_then(FinalizeCallKind::from_identifier)
        {
            Some(FinalizeCallKind::Success) => Ok(FinalizeResult::Success(arguments.get("output").cloned().unwrap_or(Value::Null))),
            Some(FinalizeCallKind::Fail) => Ok(FinalizeResult::Fail(
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

    fn argument_error(&self, message: String, schema_cache: &mut ModelSchemaCache) -> Value {
        json!({
            "error": "tool_argument_schema_mismatch",
            "tool_name": self.name,
            "message": message,
            "expected_schema": self.input_schema.json_value_with_cache(schema_cache),
        })
    }

    fn call_limit_error(&self, message: String) -> Value {
        json!({
            "error": "tool_call_limit_exceeded",
            "tool_name": self.name,
            "message": format!("{message}. Do not call this tool again; continue with the available information or choose another allowed action."),
            "max_calls": self.max_calls,
        })
    }

    fn execute_external_tool(
        &self,
        request: &ModelRequest,
        arguments: Value,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        match &self.source {
            ModelToolSource::Mcp {
                server_name,
                tool_name,
                endpoint,
                headers,
            } => self.execute_mcp_tool(
                request,
                arguments,
                McpToolTarget {
                    server_name: server_name.as_deref(),
                    tool_name,
                    endpoint,
                    headers,
                },
                schema_cache,
            ),
            ModelToolSource::McpPrompt {
                server_name,
                prompt_name,
                endpoint,
                headers,
            } => self.execute_mcp_prompt(
                request,
                arguments,
                McpImportTarget {
                    server_name,
                    item_name: prompt_name,
                    endpoint,
                    headers,
                },
                schema_cache,
            ),
            ModelToolSource::McpResource {
                server_name,
                resource_name,
                endpoint,
                headers,
            } => self.execute_mcp_resource(
                request,
                arguments,
                McpImportTarget {
                    server_name,
                    item_name: resource_name,
                    endpoint,
                    headers,
                },
                schema_cache,
            ),
            ModelToolSource::Finalize => unreachable!("finalize tool calls should return before MCP dispatch"),
            ModelToolSource::Local => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("tool `{}` is not backed by MCP", self.name),
            }),
        }
    }

    fn execute_mcp_tool(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpToolTarget<'_>,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        let server_config = McpServerConfig {
            name: target.server_name.unwrap_or("default").to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::new(
            "call".to_string(),
            self.name.clone(),
            server_config.name.clone(),
            target.tool_name.to_string(),
            arguments.clone(),
            Some(self.input_schema.json_value_with_cache(schema_cache)),
        );

        request.send_mcp_call_started(&call_details);
        let started_at = Instant::now();
        log::info!(
            "dispatching MCP tool call: agent={}, tool={}, mcp_tool={}",
            request.agent_name,
            self.name,
            target.tool_name
        );

        let result = match request.mcp_pool.get(&server_config)?.call_tool(target.tool_name, arguments) {
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
        let projected_result = self.output_schema.project_json_value(&normalized_result);

        request.send_mcp_call_completed(call_details, projected_result.clone(), result, started_at.elapsed());
        log::debug!("completed MCP tool call: agent={}, tool={}", request.agent_name, self.name);

        Ok(ToolCallOutcome::Continue(projected_result))
    }

    fn execute_mcp_prompt(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        let server_config = McpServerConfig {
            name: target.server_name.to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::new(
            "render".to_string(),
            self.name.clone(),
            server_config.name.clone(),
            target.item_name.to_string(),
            arguments.clone(),
            Some(self.input_schema.json_value_with_cache(schema_cache)),
        );

        request.send_mcp_call_started(&call_details);
        let started_at = Instant::now();
        let result = match request.mcp_pool.get(&server_config)?.get_prompt(target.item_name, arguments) {
            Ok(result) => result,
            Err(error) => {
                request.send_mcp_call_failed(call_details, Value::String(error.to_string()), started_at.elapsed());

                return Err(ExecutorError::Model {
                    agent_name: request.agent_name.clone(),
                    message: error.to_string(),
                });
            }
        };
        let rendered_result = Value::String(result.render_mcp_prompt_result());

        request.send_mcp_call_completed(call_details, rendered_result.clone(), result, started_at.elapsed());

        Ok(ToolCallOutcome::Continue(rendered_result))
    }

    fn execute_mcp_resource(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ExecutorError> {
        let server_config = McpServerConfig {
            name: target.server_name.to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::new(
            "read".to_string(),
            self.name.clone(),
            server_config.name.clone(),
            target.item_name.to_string(),
            arguments.clone(),
            Some(self.input_schema.json_value_with_cache(schema_cache)),
        );

        request.send_mcp_call_started(&call_details);
        let started_at = Instant::now();
        let result = match request.mcp_pool.get(&server_config)?.read_resource(target.item_name, arguments) {
            Ok(result) => result,
            Err(error) => {
                request.send_mcp_call_failed(call_details, Value::String(error.to_string()), started_at.elapsed());

                return Err(ExecutorError::Model {
                    agent_name: request.agent_name.clone(),
                    message: error.to_string(),
                });
            }
        };
        let rendered_result = Value::String(result.render_mcp_resource_result());

        request.send_mcp_call_completed(call_details, rendered_result.clone(), result, started_at.elapsed());

        Ok(ToolCallOutcome::Continue(rendered_result))
    }
}

trait ProviderConfigCerseiExt {
    fn build_provider(&self, request: &ModelRequest) -> Result<Box<dyn Provider>, ExecutorError>;

    fn required_api_key(&self, request: &ModelRequest, api_key: Option<String>) -> Result<String, ExecutorError>;
}

impl ProviderConfigCerseiExt for ProviderConfig {
    fn build_provider(&self, request: &ModelRequest) -> Result<Box<dyn Provider>, ExecutorError> {
        let endpoint = self.endpoint.clone().or_else(|| self.driver.default_endpoint().map(str::to_string));
        let api_key = self.api_key.clone().or_else(|| self.driver.api_key_from_environment());

        log::debug!(
            "building Cersei provider: agent={}, provider={}, endpoint={}, api_key={}",
            request.agent_name,
            self.driver.as_str(),
            endpoint.as_deref().unwrap_or("default"),
            if api_key.is_some() { "configured" } else { "missing" }
        );

        match self.driver.api_format() {
            ProviderApiFormat::Anthropic => {
                let api_key = self.required_api_key(request, api_key)?;
                let mut builder = Anthropic::builder().api_key(api_key).model(request.model_name.clone());

                if let Some(endpoint) = endpoint {
                    builder = builder.base_url(endpoint);
                }

                builder
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: format!("failed to build Anthropic provider: {error}"),
                    })
            }
            ProviderApiFormat::Google => {
                let api_key = self.required_api_key(request, api_key)?;
                let mut builder = Gemini::builder().api_key(api_key).model(request.model_name.clone());

                if let Some(endpoint) = endpoint {
                    builder = builder.base_url(endpoint);
                }

                builder
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: format!("failed to build Gemini provider: {error}"),
                    })
            }
            ProviderApiFormat::OpenAiCompatible => {
                let endpoint = endpoint.ok_or_else(|| ExecutorError::Model {
                    agent_name: request.agent_name.clone(),
                    message: format!("provider `{}` requires an endpoint", self.driver.as_str()),
                })?;
                let api_key = if self.driver == ProviderDriver::Ollama {
                    api_key.unwrap_or_else(|| "no-key".to_string())
                } else {
                    self.required_api_key(request, api_key)?
                };

                OpenAi::builder()
                    .base_url(endpoint)
                    .api_key(api_key)
                    .model(request.model_name.clone())
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: format!("failed to build OpenAI-compatible provider: {error}"),
                    })
            }
        }
    }

    fn required_api_key(&self, request: &ModelRequest, api_key: Option<String>) -> Result<String, ExecutorError> {
        if !self.driver.requires_api_key() {
            return Ok(api_key.unwrap_or_else(|| "no-key".to_string()));
        }

        api_key.ok_or_else(|| ExecutorError::Model {
            agent_name: request.agent_name.clone(),
            message: format!(
                "provider `{}` requires `api_key` or one of these environment variables: {}",
                self.driver.as_str(),
                self.driver.api_key_environment_variables().join(", ")
            ),
        })
    }
}

trait ProviderDriverRuntimeExt {
    fn api_key_from_environment(self) -> Option<String>;
}

impl ProviderDriverRuntimeExt for ProviderDriver {
    fn api_key_from_environment(self) -> Option<String> {
        self.api_key_environment_variables().iter().find_map(|environment_variable| {
            std::env::var(environment_variable)
                .ok()
                .filter(|environment_value| !environment_value.is_empty())
        })
    }
}

trait MessageExt {
    fn non_empty_text(&self) -> Option<String>;

    fn without_empty_text_blocks(self) -> Self;
}

impl MessageExt for Message {
    fn non_empty_text(&self) -> Option<String> {
        let content = self.get_all_text();
        let trimmed_content = content.trim();

        (!trimmed_content.is_empty()).then(|| trimmed_content.to_string())
    }

    fn without_empty_text_blocks(self) -> Self {
        let Message {
            role,
            content,
            id,
            metadata,
        } = self;
        let MessageContent::Blocks(content_blocks) = content else {
            return Self {
                role,
                content,
                id,
                metadata,
            };
        };
        let filtered_blocks = content_blocks
            .into_iter()
            .filter(|content_block| match content_block {
                ContentBlock::Text { text } => !text.trim().is_empty(),
                _ => true,
            })
            .collect::<Vec<_>>();

        Self {
            role,
            content: MessageContent::Blocks(filtered_blocks),
            id,
            metadata,
        }
    }
}

trait McpRenderedValue {
    fn render_mcp_prompt_result(&self) -> String;

    fn render_mcp_resource_result(&self) -> String;
}

impl McpRenderedValue for Value {
    fn render_mcp_prompt_result(&self) -> String {
        self.get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|message| message.pointer("/content/text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|content| !content.is_empty())
            .unwrap_or_else(|| self.to_string())
    }

    fn render_mcp_resource_result(&self) -> String {
        self.get("contents")
            .and_then(Value::as_array)
            .map(|contents| {
                contents
                    .iter()
                    .filter_map(|content| content.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|content| !content.is_empty())
            .unwrap_or_else(|| self.to_string())
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
    let validation_message = validation_error.masked().to_string();

    if instance_path == "$" {
        return format!("{instance_path}: {validation_message}");
    }

    format!("{instance_path}: {validation_message}")
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
    fn send_tool_call_failed(&self, tool_name: &str, error: &Value, duration: Duration);

    fn send_mcp_tool_validation_started(&self, tool_name: &str, arguments: &Value, input_schema: &Value);

    fn send_mcp_tool_validation_failed(&self, tool_name: &str, error: &Value, duration: Duration);

    fn send_mcp_tool_validation_completed(&self, tool_name: &str, duration: Duration);

    fn send_mcp_call_started(&self, details: &McpCallEventDetails);

    fn send_mcp_call_failed(&self, details: McpCallEventDetails, error: Value, duration: Duration);

    fn send_mcp_call_completed(&self, details: McpCallEventDetails, result: Value, raw_result: Value, duration: Duration);
}

impl ToolCallEventSender for ModelRequest {
    fn send_tool_call_failed(&self, tool_name: &str, error: &Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::tool_call_failed(
                self.agent_name.clone(),
                tool_name.to_string(),
                error.clone(),
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_started(&self, tool_name: &str, arguments: &Value, input_schema: &Value) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_started(
                self.agent_name.clone(),
                tool_name.to_string(),
                arguments.clone(),
                input_schema.clone(),
            ));
        }
    }

    fn send_mcp_tool_validation_failed(&self, tool_name: &str, error: &Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_failed(
                self.agent_name.clone(),
                tool_name.to_string(),
                error.clone(),
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_completed(&self, tool_name: &str, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_tool_validation_completed(
                self.agent_name.clone(),
                tool_name.to_string(),
                duration,
            ));
        }
    }

    fn send_mcp_call_started(&self, details: &McpCallEventDetails) {
        if let Some(event_sender) = &self.event_sender {
            let _ = event_sender.try_send(ExecutorEvent::mcp_call_started(details.clone()).with_agent_name(self.agent_name.clone()));
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
