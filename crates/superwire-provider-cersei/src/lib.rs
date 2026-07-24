mod network;

pub use network::{ProviderNetworkPolicy, ProviderNetworkPolicyParseError};

use async_trait::async_trait;
use cersei_provider::{Anthropic, CompletionRequest, Gemini, OpenAi, Provider};
use cersei_types::{
    CerseiError, CitationsConfig, ContentBlock, DocumentSource, ImageSource, Message, MessageContent, ToolDefinition, ToolResultContent,
};
use jsonschema::ValidationError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use superwire_mcp::{normalize_mcp_tool_result, render_mcp_prompt_text_result, render_mcp_resource_text_result, McpError, McpServerConfig};
use superwire_model::{
    ExecutorEventSenderExt, FinalizeCallKind, ModelAsset, ModelAssetSource, ModelFileAttachment, ModelPromptContent, ModelProvider,
    ModelProviderError as ProviderError, ModelRequest, ModelResponse, ModelSchema, ModelSchemaCache, ModelToolDefinition, ModelToolSource,
};
use superwire_protocol::event::{
    DiagnosticRetryability, ExecutorDiagnostic, ExecutorDiagnosticCode, ExecutorDiagnosticSubject, ExecutorEvent, ExecutorStage,
    McpCallEventDetails, McpOperation,
};
use superwire_semantic::support::provider::{ProviderApiFormat, ProviderConfig, ProviderDriver};
use superwire_types::{ModelAssetKind, ModelWireApi};

const MAX_TOOL_CALL_ROUNDS: usize = 8;
const DEFAULT_MAX_TOKENS: u32 = 16_384;
const CONTEXT_COMPACTION_AGENT_SUFFIX: &str = "__context_compaction";
const DEFAULT_PROVIDER_MAX_RETRIES: u32 = 3;
const DEFAULT_PROVIDER_RETRY_BASE_DELAY_MS: u64 = 1000;
const MAX_PROVIDER_RETRIES: u32 = 8;
const MAX_PROVIDER_RETRY_BASE_DELAY_MS: u64 = 60_000;
const MAX_PROVIDER_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct ProviderRetryContext<'request> {
    request: &'request ModelRequest,
}

impl<'request> ProviderRetryContext<'request> {
    fn new(request: &'request ModelRequest) -> Self {
        Self { request }
    }

    fn max_retries(self) -> Result<u32, ProviderError> {
        let configured_retries = match self.request.inference.get(InferenceParameter::ProviderMaxRetries.as_str()) {
            Some(value) => value.as_u64().ok_or_else(|| {
                self.invalid_configuration(format!(
                    "`{}` must be a non-negative integer",
                    InferenceParameter::ProviderMaxRetries.as_str()
                ))
            })?,
            None => u64::from(DEFAULT_PROVIDER_MAX_RETRIES),
        };
        let max_retries = u32::try_from(configured_retries).map_err(|_| {
            self.invalid_configuration(format!(
                "`{}` must be at most {MAX_PROVIDER_RETRIES}",
                InferenceParameter::ProviderMaxRetries.as_str()
            ))
        })?;

        if max_retries > MAX_PROVIDER_RETRIES {
            return Err(self.invalid_configuration(format!(
                "`{}` must be at most {MAX_PROVIDER_RETRIES}, found {max_retries}",
                InferenceParameter::ProviderMaxRetries.as_str()
            )));
        }

        Ok(max_retries)
    }

    fn base_delay(self) -> Result<Duration, ProviderError> {
        let milliseconds = match self.request.inference.get(InferenceParameter::ProviderRetryBaseDelayMs.as_str()) {
            Some(value) => value.as_u64().ok_or_else(|| {
                self.invalid_configuration(format!(
                    "`{}` must be a non-negative integer",
                    InferenceParameter::ProviderRetryBaseDelayMs.as_str()
                ))
            })?,
            None => DEFAULT_PROVIDER_RETRY_BASE_DELAY_MS,
        };

        if milliseconds > MAX_PROVIDER_RETRY_BASE_DELAY_MS {
            return Err(self.invalid_configuration(format!(
                "`{}` must be at most {MAX_PROVIDER_RETRY_BASE_DELAY_MS}, found {milliseconds}",
                InferenceParameter::ProviderRetryBaseDelayMs.as_str()
            )));
        }

        Ok(Duration::from_millis(milliseconds))
    }

    fn send_attempt_started(self, attempt: u32, total_attempts: u32) {
        if let Some(event_sender) = &self.request.event_sender {
            event_sender.try_send_observed(ExecutorEvent::provider_attempt_started(
                self.request.agent_name.clone(),
                self.request.provider_config.driver.as_str().to_string(),
                self.request.model_name.clone(),
                attempt,
                total_attempts,
            ));
        }
    }

    fn send_attempt_completed(self, attempt: u32, total_attempts: u32, duration: Duration) {
        if let Some(event_sender) = &self.request.event_sender {
            event_sender.try_send_observed(ExecutorEvent::provider_attempt_completed(
                self.request.agent_name.clone(),
                self.request.provider_config.driver.as_str().to_string(),
                self.request.model_name.clone(),
                attempt,
                total_attempts,
                duration,
            ));
        }
    }

    fn send_attempt_failed(self, attempt: u32, total_attempts: u32, diagnostic: ExecutorDiagnostic) {
        if let Some(event_sender) = &self.request.event_sender {
            event_sender.try_send_observed(ExecutorEvent::provider_attempt_failed(
                self.request.agent_name.clone(),
                self.request.provider_config.driver.as_str().to_string(),
                self.request.model_name.clone(),
                attempt,
                total_attempts,
                diagnostic,
            ));
        }
    }

    fn failure_diagnostic(self, error: &CerseiError, attempt: u32) -> ExecutorDiagnostic {
        let (code, retryability, retry_after, http_status) = match error {
            CerseiError::RateLimit { retry_after } => (
                ExecutorDiagnosticCode::ProviderRateLimited,
                DiagnosticRetryability::AfterDelay,
                *retry_after,
                Some(429),
            ),
            CerseiError::ProviderStatus { status, message: _ } => {
                let retryability = if Self::retryable_http_status(*status) {
                    DiagnosticRetryability::Safe
                } else {
                    DiagnosticRetryability::Never
                };
                let code = if *status == 429 {
                    ExecutorDiagnosticCode::ProviderRateLimited
                } else {
                    ExecutorDiagnosticCode::ModelProviderFailed
                };

                (code, retryability, None, Some(*status))
            }
            CerseiError::Provider(message) => {
                let http_status = Self::provider_message_http_status(message);
                let retryability = if http_status.is_some_and(Self::retryable_http_status) {
                    DiagnosticRetryability::Safe
                } else {
                    DiagnosticRetryability::Never
                };
                let code = if http_status == Some(429) {
                    ExecutorDiagnosticCode::ProviderRateLimited
                } else {
                    ExecutorDiagnosticCode::ModelProviderFailed
                };

                (code, retryability, None, http_status)
            }
            CerseiError::Http(error) if error.is_timeout() || error.is_connect() || error.is_request() => (
                ExecutorDiagnosticCode::ModelProviderFailed,
                DiagnosticRetryability::Safe,
                None,
                error.status().map(|status| status.as_u16()),
            ),
            CerseiError::Io(_) => (
                ExecutorDiagnosticCode::ModelProviderFailed,
                DiagnosticRetryability::Safe,
                None,
                None,
            ),
            CerseiError::Cancelled => (ExecutorDiagnosticCode::Cancelled, DiagnosticRetryability::Never, None, None),
            CerseiError::Auth(_)
            | CerseiError::Tool(_)
            | CerseiError::Permission(_)
            | CerseiError::ContextOverflow { .. }
            | CerseiError::Config(_)
            | CerseiError::Mcp(_)
            | CerseiError::Json(_)
            | CerseiError::Http(_)
            | CerseiError::Other(_) => (
                ExecutorDiagnosticCode::ModelProviderFailed,
                DiagnosticRetryability::Never,
                None,
                None,
            ),
        };
        let diagnostic = ExecutorDiagnostic::error(
            code,
            ExecutorStage::Model,
            Self::safe_failure_message(error, http_status),
            ExecutorDiagnosticSubject::Provider {
                agent_name: self.request.agent_name.clone(),
                provider_name: Some(self.request.provider_config.driver.as_str().to_string()),
                model_name: Some(self.request.model_name.clone()),
                attempt: Some(attempt),
                http_status,
            },
        )
        .with_retryability(retryability);

        match retry_after {
            Some(retry_after) => diagnostic.with_retry_after(retry_after),
            None => diagnostic,
        }
    }

    fn safe_failure_message(error: &CerseiError, http_status: Option<u16>) -> &'static str {
        match error {
            CerseiError::RateLimit { .. } => "provider rate limit exceeded",
            CerseiError::ProviderStatus { .. } | CerseiError::Provider(_) => Self::safe_http_status_message(http_status),
            CerseiError::Http(error) if error.is_timeout() => "provider request timed out",
            CerseiError::Http(error) if error.is_connect() => "provider connection failed",
            CerseiError::Http(_) => "provider HTTP request failed",
            CerseiError::Io(_) => "provider I/O operation failed",
            CerseiError::Cancelled => "provider request was cancelled",
            CerseiError::Auth(_) => "provider authentication failed",
            CerseiError::Tool(_) => "provider tool processing failed",
            CerseiError::Permission(_) => "provider permission denied",
            CerseiError::ContextOverflow { .. } => "provider context limit exceeded",
            CerseiError::Config(_) => "provider configuration was rejected",
            CerseiError::Mcp(_) => "provider MCP operation failed",
            CerseiError::Json(_) => "provider response was invalid JSON",
            CerseiError::Other(_) => "provider request failed",
        }
    }

    fn safe_http_status_message(http_status: Option<u16>) -> &'static str {
        match http_status {
            Some(401) => "provider authentication failed",
            Some(403) => "provider permission denied",
            Some(429) => "provider rate limit exceeded",
            Some(400..=499) => "provider rejected the request",
            Some(500..=599) => "provider service failed",
            Some(_) | None => "provider request failed",
        }
    }

    fn invalid_configuration(self, message: String) -> ProviderError {
        ProviderError::from_diagnostic(ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::InvalidConfiguration,
            ExecutorStage::Model,
            message,
            ExecutorDiagnosticSubject::Provider {
                agent_name: self.request.agent_name.clone(),
                provider_name: Some(self.request.provider_config.driver.as_str().to_string()),
                model_name: Some(self.request.model_name.clone()),
                attempt: None,
                http_status: None,
            },
        ))
    }

    fn retry_delay(self, base_delay: Duration, attempt_index: u32) -> Result<Duration, ProviderError> {
        let multiplier = 2_u32
            .checked_pow(attempt_index)
            .ok_or_else(|| self.invalid_configuration("provider retry multiplier overflowed".to_string()))?;
        let delay = base_delay
            .checked_mul(multiplier)
            .ok_or_else(|| self.invalid_configuration("provider retry delay overflowed".to_string()))?;

        Ok(delay.min(MAX_PROVIDER_RETRY_DELAY))
    }

    fn retryable_http_status(status: u16) -> bool {
        matches!(status, 408 | 425 | 429 | 500..=599)
    }

    fn provider_message_http_status(message: &str) -> Option<u16> {
        message
            .strip_prefix("HTTP ")
            .and_then(|status_and_body| {
                status_and_body
                    .split_once(':')
                    .map_or(Some(status_and_body), |(status, _)| Some(status))
            })
            .and_then(|status| status.trim().parse().ok())
    }
}

struct ProviderAttemptLifecycle<'request> {
    retry_context: ProviderRetryContext<'request>,
    attempt: u32,
    total_attempts: u32,
    started_at: Instant,
    terminal: bool,
}

impl<'request> ProviderAttemptLifecycle<'request> {
    fn new(retry_context: ProviderRetryContext<'request>, attempt: u32, total_attempts: u32) -> Self {
        let started_at = Instant::now();
        retry_context.send_attempt_started(attempt, total_attempts);

        Self {
            retry_context,
            attempt,
            total_attempts,
            started_at,
            terminal: false,
        }
    }

    fn complete(&mut self) {
        self.retry_context
            .send_attempt_completed(self.attempt, self.total_attempts, self.started_at.elapsed());
        self.terminal = true;
    }

    fn fail(&mut self, diagnostic: ExecutorDiagnostic) {
        self.retry_context
            .send_attempt_failed(self.attempt, self.total_attempts, diagnostic);
        self.terminal = true;
    }
}

impl Drop for ProviderAttemptLifecycle<'_> {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }

        let (diagnostic_code, message) = if std::thread::panicking() {
            (
                ExecutorDiagnosticCode::InternalPanic,
                format!(
                    "provider attempt {}/{} panicked before a terminal event",
                    self.attempt, self.total_attempts
                ),
            )
        } else {
            (
                ExecutorDiagnosticCode::Cancelled,
                format!(
                    "provider attempt {}/{} was cancelled before a terminal event",
                    self.attempt, self.total_attempts
                ),
            )
        };
        let diagnostic = ExecutorDiagnostic::error(
            diagnostic_code,
            ExecutorStage::Model,
            message,
            ExecutorDiagnosticSubject::Provider {
                agent_name: self.retry_context.request.agent_name.clone(),
                provider_name: Some(self.retry_context.request.provider_config.driver.as_str().to_string()),
                model_name: Some(self.retry_context.request.model_name.clone()),
                attempt: Some(self.attempt),
                http_status: None,
            },
        )
        .with_retryability(DiagnosticRetryability::Never);

        self.fail(diagnostic);
    }
}

#[derive(Debug, Clone)]
pub struct CerseiModelProvider {
    network_policy: ProviderNetworkPolicy,
    dns_resolver: Arc<dyn network::ProviderDnsResolver>,
}

impl CerseiModelProvider {
    #[must_use]
    pub fn for_network_policy(network_policy: ProviderNetworkPolicy) -> Self {
        Self {
            network_policy,
            dns_resolver: Arc::new(network::SystemProviderDnsResolver),
        }
    }

    #[cfg(test)]
    fn for_network_policy_and_dns_resolver(
        network_policy: ProviderNetworkPolicy,
        dns_resolver: Arc<dyn network::ProviderDnsResolver>,
    ) -> Self {
        Self {
            network_policy,
            dns_resolver,
        }
    }

    async fn approve_endpoint(&self, request: &ModelRequest) -> Result<network::ProviderEndpointApproval, ProviderError> {
        self.network_policy
            .approve(&request.provider_config, request, self.dns_resolver.as_ref())
            .await
    }
}

impl Default for CerseiModelProvider {
    fn default() -> Self {
        Self::for_network_policy(ProviderNetworkPolicy::BuiltInOnly)
    }
}

#[async_trait]
impl ModelProvider for CerseiModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ProviderError> {
        let endpoint_approval = self.approve_endpoint(&request).await?;
        let uploaded_files = self.upload_files(&request, &endpoint_approval).await?;
        let file_upload_client = if uploaded_files.is_empty() {
            None
        } else {
            Some(FileUploadClient::from_request(&request, &endpoint_approval)?)
        };
        let uploaded_file_ids = uploaded_files.iter().map(|uploaded_file| uploaded_file.id.clone()).collect();
        let mut cleanup_guard = UploadedProviderFileCleanup::new(file_upload_client, request.agent_name.clone(), uploaded_file_ids);
        let generation_result = self
            .generate_with_uploaded_files(&request, &endpoint_approval, &uploaded_files)
            .await;
        let cleanup_result = cleanup_guard.cleanup_now(&request, &uploaded_files).await;
        match (generation_result, cleanup_result) {
            (Ok(model_response), Ok(())) => Ok(model_response),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
            (Err(error), Err(cleanup_error)) => Err(error.with_cause(cleanup_error.diagnostic().clone())),
        }
    }
}

impl CerseiModelProvider {
    async fn generate_with_uploaded_files(
        &self,
        request: &ModelRequest,
        endpoint_approval: &network::ProviderEndpointApproval,
        uploaded_files: &[UploadedProviderFile],
    ) -> Result<ModelResponse, ProviderError> {
        if request.should_generate_file_attachments_without_tools() {
            return self
                .generate_file_response_with_uploaded_files(request, endpoint_approval, uploaded_files)
                .await;
        }

        let provider = request.provider_config.build_provider(request, endpoint_approval)?;
        let mut schema_cache = ModelSchemaCache::new();
        let request_context = request.cersei_request_context(&mut schema_cache)?;
        let context_messages = request.cersei_context_messages(uploaded_files)?;
        let mut messages = context_messages.clone();
        let retry_context = ProviderRetryContext::new(request);
        let max_retries = retry_context.max_retries()?;
        let retry_base_delay = retry_context.base_delay()?;

        log::info!(
            "starting Cersei generation: agent={}, provider={}, model={}, tools={}, max_retries={}",
            request.agent_name,
            request.provider_config.driver.as_str(),
            request.model_name,
            request.tools.len(),
            max_retries
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

            let completion =
                Self::complete_with_retry(&provider, &completion_request, retry_context, max_retries, retry_base_delay).await?;

            let tool_calls = CerseiToolCall::from_message(&completion.message);

            if !tool_calls.is_empty() {
                log::info!(
                    "provider requested tool calls: agent={}, provider={}, count={}",
                    request.agent_name,
                    request.provider_config.driver.as_str(),
                    tool_calls.len()
                );
                let tool_call_round = self.execute_tool_calls(request, &tool_calls, &mut schema_cache)?;

                if let Some(finalize_result) = tool_call_round.finalize_result {
                    return self.complete_generation(request, &context_messages, finalize_result);
                }

                messages.push(completion.message.without_empty_text_blocks());
                messages.extend(tool_call_round.messages);

                continue;
            }

            messages.push(completion.message);
            messages.push(Message::user(
                "To finish this agent run you must call the internal `finalize` tool. Call `finalize` with ` {\"type\":\"success\",\"output\":...}` when the output is ready and matches the schema, or `{\"type\":\"fail\",\"reason\":\"...\"}` when you cannot fulfill the request. Do not answer with plain text.",
            ));
        }

        Err(ProviderError::model(
            request.agent_name.clone(),
            "model did not call the required finalize tool",
        ))
    }

    async fn generate_file_response_with_uploaded_files(
        &self,
        request: &ModelRequest,
        endpoint_approval: &network::ProviderEndpointApproval,
        uploaded_files: &[UploadedProviderFile],
    ) -> Result<ModelResponse, ProviderError> {
        let provider = request.provider_config.build_provider(request, endpoint_approval)?;
        let mut schema_cache = ModelSchemaCache::new();
        let context_messages = request.cersei_context_messages(uploaded_files)?;
        let output_schema_text = request.output_schema.json_string_with_cache(&mut schema_cache).map_err(|error| {
            ProviderError::model_with_source(request.agent_name.clone(), "failed to serialize the model output schema", error)
        })?;
        let mut completion_request = CompletionRequest::new(request.model_name.clone());
        let retry_context = ProviderRetryContext::new(request);
        let max_retries = retry_context.max_retries()?;
        let retry_base_delay = retry_context.base_delay()?;

        completion_request.system = Some(format!(
            "You are executing a deterministic workflow agent. Uploaded files may appear as `fileid://...` system messages in this conversation. Answer the user's instruction using those files. Return only a JSON value matching this JSON Schema: {output_schema_text}. Do not call tools."
        ));
        completion_request.messages = context_messages.clone();
        completion_request.max_tokens = request.max_tokens();
        completion_request.temperature = request.temperature();

        log::info!(
            "starting Cersei file generation without tools: agent={}, provider={}, model={}, max_retries={}",
            request.agent_name,
            request.provider_config.driver.as_str(),
            request.model_name,
            max_retries
        );

        let completion = Self::complete_with_retry(&provider, &completion_request, retry_context, max_retries, retry_base_delay).await?;
        let response_text = completion
            .message
            .non_empty_text()
            .ok_or_else(|| ProviderError::model(request.agent_name.clone(), "file response did not include text".to_string()))?;
        let output = request
            .output_schema
            .parse_chat_completion_text_output(&response_text, &request.agent_name, &mut schema_cache)?;
        let mut messages = context_messages;

        messages.push(Message::assistant(response_text));

        Ok(ModelResponse {
            output,
            context: CerseiAgentContext { messages }.into_value(),
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

struct CerseiAgentContext {
    messages: Vec<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CerseiAgentContextField {
    Marker,
    Messages,
}

impl CerseiAgentContextField {
    fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "__superwire_cersei_context",
            Self::Messages => "messages",
        }
    }
}

impl CerseiAgentContext {
    fn from_value(value: &Value, agent_name: &str) -> Result<Self, ProviderError> {
        if value.get(CerseiAgentContextField::Marker.as_str()).and_then(Value::as_bool) != Some(true) {
            return Err(ProviderError::model(
                agent_name.to_string(),
                "agent context was not produced by the Cersei provider".to_string(),
            ));
        }

        let messages_value = value
            .get(CerseiAgentContextField::Messages.as_str())
            .cloned()
            .ok_or_else(|| ProviderError::model(agent_name.to_string(), "agent context does not include messages".to_string()))?;
        let messages = serde_json::from_value(messages_value)
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), "agent context messages are invalid", error))?;

        Ok(Self { messages })
    }

    fn into_value(self) -> Value {
        let mut context_object = serde_json::Map::new();
        context_object.insert(CerseiAgentContextField::Marker.as_str().to_string(), Value::Bool(true));
        context_object.insert(
            CerseiAgentContextField::Messages.as_str().to_string(),
            serde_json::to_value(self.messages).unwrap_or(Value::Null),
        );

        Value::Object(context_object)
    }

    fn from_compaction_summary(summary: String) -> Self {
        Self {
            messages: vec![Message::user(summary)],
        }
    }
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

trait ModelRequestCerseiMessageExt {
    fn cersei_context_messages(&self, uploaded_files: &[UploadedProviderFile]) -> Result<Vec<Message>, ProviderError>;
    fn cersei_user_message(&self) -> Message;
    fn cersei_content_blocks(&self) -> Vec<ContentBlock>;
}

impl ModelRequestCerseiMessageExt for ModelRequest {
    fn cersei_context_messages(&self, uploaded_files: &[UploadedProviderFile]) -> Result<Vec<Message>, ProviderError> {
        let mut messages = if let Some(context_value) = &self.context {
            CerseiAgentContext::from_value(context_value, &self.agent_name)?.messages
        } else {
            Vec::new()
        };

        for uploaded_file in uploaded_files {
            messages.push(Message::system(format!("fileid://{}", uploaded_file.id)));
        }

        messages.push(self.cersei_user_message());

        Ok(messages)
    }

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

trait ModelAssetCerseiExt {
    fn cersei_content_block(&self) -> ContentBlock;
    fn cersei_image_source(&self) -> ImageSource;
    fn cersei_document_source(&self) -> DocumentSource;
    fn cersei_source_parts(&self) -> (String, Option<String>, Option<String>);
}

impl ModelAssetCerseiExt for ModelAsset {
    fn cersei_content_block(&self) -> ContentBlock {
        match self.kind {
            ModelAssetKind::Image => ContentBlock::Image {
                source: self.cersei_image_source(),
            },
            ModelAssetKind::Document | ModelAssetKind::Video => ContentBlock::Document {
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

trait ModelFileAttachmentsCerseiExt {
    fn cersei_provider_uploads(&self) -> Vec<ModelFileAttachment>;

    fn cersei_bundled_provider_upload(&self) -> ModelFileAttachment;
}

impl ModelFileAttachmentsCerseiExt for [ModelFileAttachment] {
    fn cersei_provider_uploads(&self) -> Vec<ModelFileAttachment> {
        if self.len() <= 1 {
            return self.to_vec();
        }

        vec![self.cersei_bundled_provider_upload()]
    }

    fn cersei_bundled_provider_upload(&self) -> ModelFileAttachment {
        let mut content = String::from("Superwire uploaded files\n");

        for (file_index, file_attachment) in self.iter().enumerate() {
            content.push('\n');
            content.push_str("File ");
            content.push_str(&(file_index + 1).to_string());
            content.push_str(": ");
            content.push_str(&file_attachment.name);
            content.push('\n');
            content.push_str("Purpose: ");
            content.push_str(&file_attachment.purpose);
            content.push('\n');
            content.push_str("Content:\n");
            content.push_str(&file_attachment.content);
            content.push('\n');
        }

        ModelFileAttachment {
            name: "superwire-files.txt".to_string(),
            content,
            purpose: self
                .first()
                .map_or_else(|| "file-extract".to_string(), |file_attachment| file_attachment.purpose.clone()),
        }
    }
}

impl CerseiModelProvider {
    async fn upload_files(
        &self,
        request: &ModelRequest,
        endpoint_approval: &network::ProviderEndpointApproval,
    ) -> Result<Vec<UploadedProviderFile>, ProviderError> {
        if request.file_attachments.is_empty() {
            return Ok(Vec::new());
        }

        if request.wire_api != ModelWireApi::ChatCompletion {
            return Err(ProviderError::model(
                request.agent_name.clone(),
                format!(
                    "agent `{}` file directives require `wire_api: \"{}\"`",
                    request.agent_name,
                    ModelWireApi::ChatCompletion.as_str()
                ),
            ));
        }

        if request.provider_config.driver.api_format() != ProviderApiFormat::OpenAiCompatible {
            return Err(ProviderError::model(
                request.agent_name.clone(),
                "file directives require an OpenAI-compatible provider".to_string(),
            ));
        }

        let upload_client = FileUploadClient::from_request(request, endpoint_approval)?;
        let file_uploads = request.file_attachments.as_slice().cersei_provider_uploads();
        let mut uploaded_files = Vec::new();

        for file_attachment in &file_uploads {
            match upload_client.upload(file_attachment, &request.agent_name).await {
                Ok(uploaded_file) => {
                    request.send_agent_file_created(&uploaded_file);
                    uploaded_files.push(uploaded_file);
                }
                Err(error) => {
                    let cleanup_result = upload_client.delete_uploaded_files(request, &uploaded_files).await;

                    return match cleanup_result {
                        Ok(()) => Err(error),
                        Err(cleanup_error) => Err(error.with_cause(cleanup_error.diagnostic().clone())),
                    };
                }
            }
        }

        Ok(uploaded_files)
    }

    async fn complete_with_retry(
        provider: &dyn Provider,
        request: &CompletionRequest,
        retry_context: ProviderRetryContext<'_>,
        max_retries: u32,
        base_delay: Duration,
    ) -> Result<cersei_provider::CompletionResponse, ProviderError> {
        let total_attempts = max_retries
            .checked_add(1)
            .ok_or_else(|| retry_context.invalid_configuration("provider retry attempt count overflowed".to_string()))?;

        for attempt_index in 0..total_attempts {
            let attempt = attempt_index + 1;

            let mut attempt_lifecycle = ProviderAttemptLifecycle::new(retry_context, attempt, total_attempts);

            match provider.complete_blocking(request.clone()).await {
                Ok(completion) => {
                    if attempt_index > 0 {
                        log::info!(
                            "provider request succeeded after retry: agent={}, attempt={}/{}",
                            retry_context.request.agent_name,
                            attempt,
                            total_attempts
                        );
                    }

                    attempt_lifecycle.complete();

                    return Ok(completion);
                }
                Err(error) => {
                    let diagnostic = retry_context.failure_diagnostic(&error, attempt);

                    log::warn!(
                        "provider request failed: agent={}, attempt={}/{}, code={:?}, retryability={:?}",
                        retry_context.request.agent_name,
                        attempt,
                        total_attempts,
                        diagnostic.code,
                        diagnostic.retryability
                    );
                    attempt_lifecycle.fail(diagnostic.clone());

                    let can_retry = attempt_index < max_retries
                        && matches!(
                            diagnostic.retryability,
                            DiagnosticRetryability::Safe | DiagnosticRetryability::AfterDelay
                        );

                    if !can_retry {
                        let exhausted_diagnostic = if attempt_index == max_retries && max_retries > 0 {
                            ExecutorDiagnostic::error(
                                ExecutorDiagnosticCode::ProviderRetriesExhausted,
                                ExecutorStage::Model,
                                format!("provider request failed after {total_attempts} attempts"),
                                diagnostic.subject.clone(),
                            )
                            .with_retryability(diagnostic.retryability)
                            .with_cause(diagnostic)
                        } else {
                            diagnostic
                        };

                        return Err(ProviderError::with_source(exhausted_diagnostic, error));
                    }

                    let delay = diagnostic.retry_after_ms.map(Duration::from_millis).map_or_else(
                        || retry_context.retry_delay(base_delay, attempt_index),
                        |delay| Ok(delay.min(MAX_PROVIDER_RETRY_DELAY)),
                    )?;

                    log::info!(
                        "retrying provider request: agent={}, attempt={}/{}, delay={}ms",
                        retry_context.request.agent_name,
                        attempt + 1,
                        total_attempts,
                        delay.as_millis()
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(retry_context.invalid_configuration("provider retry loop ended without a response or error".to_string()))
    }

    fn execute_tool_calls(
        &self,
        request: &ModelRequest,
        tool_calls: &[CerseiToolCall],
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallRound, ProviderError> {
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
            let tool_result_text = serde_json::to_string(&tool_result).map_err(|error| {
                ProviderError::model_with_source(request.agent_name.clone(), "failed to serialize a tool result", error)
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
    ) -> Result<ToolCallOutcome, ProviderError> {
        let tool_definition = request
            .tools
            .iter()
            .find(|tool_definition| tool_definition.name == tool_call.name)
            .ok_or_else(|| ProviderError::model(request.agent_name.clone(), "model requested an unknown tool"))?;
        let tool_call_started_at = Instant::now();

        if let Some(tool_error) = request.call_limit_error(tool_definition) {
            return Ok(ToolCallOutcome::Continue(tool_error));
        }

        log::debug!("processing model tool call: agent={}", request.agent_name);
        let mut arguments = tool_call.input.clone();
        let validation_started_at = Instant::now();
        let input_schema = tool_definition.input_schema.json_value_with_cache(schema_cache);

        if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
            request.send_mcp_tool_validation_started(&tool_definition.name, &arguments);
        }

        if let Err(message) = tool_definition.validate_value(&arguments, &input_schema, ModelToolValidationTarget::Arguments) {
            if tool_definition.is_finalize_success_with_output(&arguments) {
                return Err(ProviderError::invalid_output(
                    request.agent_name.clone(),
                    "agent finalize output does not match its declared schema",
                ));
            }

            let tool_error = tool_definition.argument_error(message, schema_cache);

            if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
                request.send_mcp_tool_validation_failed(&tool_definition.name, validation_started_at.elapsed());
            }

            request.send_tool_call_failed(&tool_definition.name, tool_call_started_at.elapsed());
            log::warn!("rejected tool call before MCP dispatch: agent={}", request.agent_name);

            return Ok(ToolCallOutcome::Continue(tool_error));
        }

        tool_definition.merge_bindings_into(&mut arguments);

        if let Some(invocation_schema) = &tool_definition.invocation_schema {
            let invocation_schema_value = invocation_schema.json_value_with_cache(schema_cache);

            if tool_definition
                .validate_value(&arguments, &invocation_schema_value, ModelToolValidationTarget::Arguments)
                .is_err()
            {
                request.send_mcp_tool_validation_failed(&tool_definition.name, validation_started_at.elapsed());
                request.send_tool_call_failed(&tool_definition.name, tool_call_started_at.elapsed());

                return Err(ProviderError::model(
                    request.agent_name.clone(),
                    format!(
                        "merged arguments for MCP tool `{}` do not match its discovered schema",
                        tool_definition.name
                    ),
                ));
            }
        }

        if matches!(tool_definition.source, ModelToolSource::Mcp { .. }) {
            request.send_mcp_tool_validation_completed(&tool_definition.name, validation_started_at.elapsed());
        }

        if matches!(tool_definition.source, ModelToolSource::Finalize) {
            return tool_definition.parse_finalize_arguments(arguments).map(ToolCallOutcome::Finalized);
        }

        tool_definition.execute_external_tool(request, arguments, schema_cache)
    }

    fn complete_generation(
        &self,
        request: &ModelRequest,
        context_messages: &[Message],
        finalize_result: FinalizeResult,
    ) -> Result<ModelResponse, ProviderError> {
        match finalize_result {
            FinalizeResult::Success(output) => {
                log::info!(
                    "Cersei generation completed: agent={}, provider={}, model={}",
                    request.agent_name,
                    request.provider_config.driver.as_str(),
                    request.model_name
                );

                let mut messages = context_messages.to_vec();
                let assistant_output = output.as_str().map_or_else(|| output.to_string(), str::to_string);

                if request.is_context_compaction() {
                    return Ok(ModelResponse {
                        output,
                        context: CerseiAgentContext::from_compaction_summary(assistant_output).into_value(),
                    });
                }

                messages.push(Message::assistant(assistant_output));

                Ok(ModelResponse {
                    output,
                    context: CerseiAgentContext { messages }.into_value(),
                })
            }
            FinalizeResult::Fail => Err(ProviderError::model(
                request.agent_name.clone(),
                "model reported that it could not complete the request",
            )),
        }
    }
}

trait ModelRequestCompactionExt {
    fn is_context_compaction(&self) -> bool;
}

impl ModelRequestCompactionExt for ModelRequest {
    fn is_context_compaction(&self) -> bool {
        self.agent_name.ends_with(CONTEXT_COMPACTION_AGENT_SUFFIX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferenceParameter {
    MaxTokens,
    Temperature,
    ProviderMaxRetries,
    ProviderRetryBaseDelayMs,
}

impl InferenceParameter {
    fn as_str(self) -> &'static str {
        match self {
            Self::MaxTokens => "max_tokens",
            Self::Temperature => "temperature",
            Self::ProviderMaxRetries => "provider_max_retries",
            Self::ProviderRetryBaseDelayMs => "provider_retry_base_delay_ms",
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
    Fail,
}

#[derive(Debug, Clone)]
struct UploadedProviderFile {
    id: String,
    filename: String,
    purpose: String,
    bytes: Option<u64>,
}

const PROVIDER_FILE_CLEANUP_MAX_IN_FLIGHT: usize = 8;
const PROVIDER_FILE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

static PROVIDER_FILE_CLEANUP_EXECUTOR: LazyLock<ProviderFileCleanupExecutor> =
    LazyLock::new(|| ProviderFileCleanupExecutor::with_limits(PROVIDER_FILE_CLEANUP_MAX_IN_FLIGHT, PROVIDER_FILE_CLEANUP_TIMEOUT));

struct UploadedProviderFileCleanup {
    file_upload_client: Option<FileUploadClient>,
    agent_name: String,
    uploaded_file_ids: Vec<String>,
    cleanup_completed: bool,
}

impl UploadedProviderFileCleanup {
    fn new(file_upload_client: Option<FileUploadClient>, agent_name: String, uploaded_file_ids: Vec<String>) -> Self {
        Self {
            file_upload_client,
            agent_name,
            uploaded_file_ids,
            cleanup_completed: false,
        }
    }

    async fn cleanup_now(&mut self, request: &ModelRequest, uploaded_files: &[UploadedProviderFile]) -> Result<(), ProviderError> {
        let cleanup_result = match &self.file_upload_client {
            Some(file_upload_client) => file_upload_client.delete_uploaded_files(request, uploaded_files).await,
            None => Ok(()),
        };

        self.cleanup_completed = true;

        cleanup_result
    }
}

impl Drop for UploadedProviderFileCleanup {
    fn drop(&mut self) {
        if self.cleanup_completed || self.uploaded_file_ids.is_empty() {
            return;
        }

        let Some(file_upload_client) = &self.file_upload_client else {
            return;
        };
        let cleanup = DetachedProviderFileCleanup {
            file_upload_client: file_upload_client.clone(),
            agent_name: self.agent_name.clone(),
            uploaded_file_ids: self.uploaded_file_ids.clone(),
        };

        match ProviderFileCleanupExecutor::shared().schedule(cleanup) {
            ProviderFileCleanupScheduleOutcome::Scheduled => {}
            ProviderFileCleanupScheduleOutcome::AtCapacity => {
                log::warn!("skipped provider file cleanup after cancellation because cleanup capacity is exhausted");
            }
            ProviderFileCleanupScheduleOutcome::RuntimeUnavailable => {
                log::warn!("skipped provider file cleanup after cancellation because the async runtime is unavailable");
            }
        }
    }
}

#[derive(Debug)]
struct ProviderFileCleanupExecutor {
    semaphore: Arc<tokio::sync::Semaphore>,
    timeout: Duration,
}

impl ProviderFileCleanupExecutor {
    fn shared() -> &'static Self {
        &PROVIDER_FILE_CLEANUP_EXECUTOR
    }

    fn with_limits(max_in_flight: usize, timeout: Duration) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_in_flight)),
            timeout,
        }
    }

    fn schedule(&self, cleanup: DetachedProviderFileCleanup) -> ProviderFileCleanupScheduleOutcome {
        let Ok(runtime_handle) = tokio::runtime::Handle::try_current() else {
            return ProviderFileCleanupScheduleOutcome::RuntimeUnavailable;
        };
        let Ok(cleanup_permit) = Arc::clone(&self.semaphore).try_acquire_owned() else {
            return ProviderFileCleanupScheduleOutcome::AtCapacity;
        };
        let cleanup_timeout = self.timeout;

        runtime_handle.spawn(async move {
            let _cleanup_permit = cleanup_permit;

            match tokio::time::timeout(cleanup_timeout, cleanup.execute()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let diagnostic = error.diagnostic();

                    log::warn!(
                        "failed to clean up provider files after cancellation: code={:?}, stage={:?}",
                        diagnostic.code,
                        diagnostic.stage
                    );
                }
                Err(_elapsed) => {
                    log::warn!("provider file cleanup after cancellation exceeded its bounded deadline");
                }
            }
        });

        ProviderFileCleanupScheduleOutcome::Scheduled
    }

    #[cfg(test)]
    fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFileCleanupScheduleOutcome {
    Scheduled,
    AtCapacity,
    RuntimeUnavailable,
}

struct DetachedProviderFileCleanup {
    file_upload_client: FileUploadClient,
    agent_name: String,
    uploaded_file_ids: Vec<String>,
}

impl DetachedProviderFileCleanup {
    async fn execute(self) -> Result<(), ProviderError> {
        let mut cleanup_error: Option<ProviderError> = None;

        for uploaded_file_id in self.uploaded_file_ids.iter().rev() {
            if let Err(error) = self.file_upload_client.delete(uploaded_file_id, &self.agent_name).await {
                cleanup_error = Some(match cleanup_error {
                    Some(existing_error) => existing_error.with_cause(error.diagnostic().clone()),
                    None => error,
                });
            }
        }

        cleanup_error.map_or(Ok(()), Err)
    }
}

#[derive(Clone, Copy)]
enum FileProviderOperation {
    Upload,
    Delete,
}

impl FileProviderOperation {
    fn failure_message(self) -> &'static str {
        match self {
            Self::Upload => "file upload failed",
            Self::Delete => "file delete failed",
        }
    }
}

#[derive(Clone)]
struct FileUploadClient {
    endpoint: String,
    api_key: String,
    provider_driver: ProviderDriver,
    model_name: String,
    client: reqwest::Client,
}

impl FileUploadClient {
    fn from_request(request: &ModelRequest, endpoint_approval: &network::ProviderEndpointApproval) -> Result<Self, ProviderError> {
        let client = endpoint_approval.http_client();
        let api_key = request.provider_config.resolved_api_key().ok_or_else(|| {
            ProviderError::model(
                request.agent_name.clone(),
                format!(
                    "provider `{}` requires an explicit api key for file uploads",
                    request.provider_config.driver.as_str()
                ),
            )
        })?;

        Ok(Self {
            endpoint: endpoint_approval.endpoint().to_string(),
            api_key,
            provider_driver: request.provider_config.driver,
            model_name: request.model_name.clone(),
            client,
        })
    }

    async fn read_bounded_body(
        &self,
        mut response: reqwest::Response,
        agent_name: &str,
        operation: FileProviderOperation,
    ) -> Result<Vec<u8>, ProviderError> {
        let initial_capacity = response
            .content_length()
            .and_then(|content_length| usize::try_from(content_length).ok())
            .unwrap_or_default()
            .min(network::PROVIDER_HTTP_MAX_RESPONSE_BODY_BYTES);
        let mut body = Vec::with_capacity(initial_capacity);

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), operation.failure_message(), error))?
        {
            let resulting_length = body.len().saturating_add(chunk.len());

            if resulting_length > network::PROVIDER_HTTP_MAX_RESPONSE_BODY_BYTES {
                return Err(ProviderError::model(
                    agent_name.to_string(),
                    "provider response body exceeded the configured limit",
                ));
            }

            body.extend_from_slice(&chunk);
        }

        Ok(body)
    }

    async fn upload(&self, file_attachment: &ModelFileAttachment, agent_name: &str) -> Result<UploadedProviderFile, ProviderError> {
        let file_part = reqwest::multipart::Part::bytes(file_attachment.content.clone().into_bytes())
            .file_name(file_attachment.name.clone())
            .mime_str("text/plain")
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), "failed to prepare file upload", error))?;
        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("purpose", file_attachment.purpose.clone());
        let url = format!("{}/files", self.endpoint.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), "file upload request failed", error))?;
        let status = response.status();

        if !status.is_success() {
            return Err(self.http_failure(agent_name, FileProviderOperation::Upload, status.as_u16()));
        }

        let body = self.read_bounded_body(response, agent_name, FileProviderOperation::Upload).await?;
        let response_value = serde_json::from_slice::<Value>(&body)
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), "file upload response was invalid JSON", error))?;
        let id = response_value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::model(agent_name.to_string(), "file upload response did not include `id`".to_string()))?
            .to_string();
        let filename = response_value
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or(file_attachment.name.as_str())
            .to_string();
        let purpose = response_value
            .get("purpose")
            .and_then(Value::as_str)
            .unwrap_or(file_attachment.purpose.as_str())
            .to_string();
        let bytes = response_value.get("bytes").and_then(Value::as_u64);

        Ok(UploadedProviderFile {
            id,
            filename,
            purpose,
            bytes,
        })
    }

    async fn delete(&self, uploaded_file_id: &str, agent_name: &str) -> Result<(), ProviderError> {
        let url = format!("{}/files/{uploaded_file_id}", self.endpoint.trim_end_matches('/'));
        let response = self
            .client
            .delete(url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|error| ProviderError::model_with_source(agent_name.to_string(), "file delete request failed", error))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();

        Err(self.http_failure(agent_name, FileProviderOperation::Delete, status.as_u16()))
    }

    async fn delete_uploaded_files(&self, request: &ModelRequest, uploaded_files: &[UploadedProviderFile]) -> Result<(), ProviderError> {
        let mut cleanup_error: Option<ProviderError> = None;

        for uploaded_file in uploaded_files.iter().rev() {
            match self.delete(&uploaded_file.id, &request.agent_name).await {
                Ok(()) => request.send_agent_file_deleted(uploaded_file),
                Err(error) => {
                    cleanup_error = Some(match cleanup_error {
                        Some(existing_error) => existing_error.with_cause(error.diagnostic().clone()),
                        None => error,
                    });
                }
            }
        }

        cleanup_error.map_or(Ok(()), Err)
    }

    fn http_failure(&self, agent_name: &str, operation: FileProviderOperation, http_status: u16) -> ProviderError {
        ProviderError::from_diagnostic(
            ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::ModelProviderFailed,
                ExecutorStage::Model,
                operation.failure_message(),
                ExecutorDiagnosticSubject::Provider {
                    agent_name: agent_name.to_string(),
                    provider_name: Some(self.provider_driver.as_str().to_string()),
                    model_name: Some(self.model_name.clone()),
                    attempt: None,
                    http_status: Some(http_status),
                },
            )
            .with_retryability(DiagnosticRetryability::Unknown),
        )
    }
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

trait ModelRequestCerseiContextExt {
    fn cersei_request_context(&self, schema_cache: &mut ModelSchemaCache) -> Result<CerseiRequestContext, ProviderError>;
    fn max_tokens(&self) -> u32;
    fn temperature(&self) -> Option<f32>;
    fn cersei_options(&self) -> HashMap<String, Value>;
    fn call_limit_error(&self, tool_definition: &ModelToolDefinition) -> Option<Value>;
    fn should_generate_file_attachments_without_tools(&self) -> bool;
}

impl ModelRequestCerseiContextExt for ModelRequest {
    fn cersei_request_context(&self, schema_cache: &mut ModelSchemaCache) -> Result<CerseiRequestContext, ProviderError> {
        let output_schema_text = self.output_schema.json_string_with_cache(schema_cache).map_err(|error| {
            ProviderError::model_with_source(self.agent_name.clone(), "failed to serialize the model output schema", error)
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
                    && setting_name.as_str() != InferenceParameter::ProviderMaxRetries.as_str()
                    && setting_name.as_str() != InferenceParameter::ProviderRetryBaseDelayMs.as_str()
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

        self.send_tool_call_failed(&tool_definition.name, Duration::ZERO);
        log::warn!(
            "rejected tool call at max_calls limit: agent={}, tool={}",
            self.agent_name,
            tool_definition.name
        );

        Some(tool_error)
    }

    fn should_generate_file_attachments_without_tools(&self) -> bool {
        !self.file_attachments.is_empty()
            && self.wire_api == ModelWireApi::ChatCompletion
            && self
                .tools
                .iter()
                .all(|tool_definition| matches!(tool_definition.source, ModelToolSource::Finalize))
    }
}

#[derive(Debug, Clone, Copy)]
enum ModelToolErrorCode {
    McpCallFailed,
}

impl ModelToolErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::McpCallFailed => "mcp_tool_call_failed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ModelToolValidationTarget {
    Arguments,
    Output,
}

impl ModelToolValidationTarget {
    fn mismatch_message(self, validation_issues: &[String]) -> String {
        match self {
            Self::Arguments => format!(
                "tool arguments do not match the declared schema: {}. Correct the arguments and call the tool again.",
                validation_issues.join("; ")
            ),
            Self::Output => format!("tool output does not match the declared schema: {}", validation_issues.join("; ")),
        }
    }
}

trait ModelToolDefinitionCerseiExt {
    fn to_cersei_tool_definition(&self, schema_cache: &mut ModelSchemaCache) -> ToolDefinition;
    fn validate_value(&self, value: &Value, schema: &Value, target: ModelToolValidationTarget) -> Result<(), String>;
    fn parse_finalize_arguments(&self, arguments: Value) -> Result<FinalizeResult, ProviderError>;
    fn is_finalize_success_with_output(&self, arguments: &Value) -> bool;
    fn argument_error(&self, message: String, schema_cache: &mut ModelSchemaCache) -> Value;
    fn call_limit_error(&self, message: String) -> Value;
    fn execute_external_tool(
        &self,
        request: &ModelRequest,
        arguments: Value,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ProviderError>;
    fn execute_mcp_tool(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpToolTarget<'_>,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ProviderError>;
    fn execute_mcp_prompt(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
    ) -> Result<ToolCallOutcome, ProviderError>;
    fn execute_mcp_resource(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
    ) -> Result<ToolCallOutcome, ProviderError>;
}

impl ModelToolDefinitionCerseiExt for ModelToolDefinition {
    fn to_cersei_tool_definition(&self, schema_cache: &mut ModelSchemaCache) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone().unwrap_or_else(|| format!("Workflow tool `{}`", self.name)),
            input_schema: self.input_schema.json_value_with_cache(schema_cache),
        }
    }

    fn validate_value(&self, value: &Value, schema: &Value, target: ModelToolValidationTarget) -> Result<(), String> {
        let validator = jsonschema::validator_for(schema).map_err(|error| format!("tool schema could not be compiled: {error}"))?;
        let mut validation_issues = validator.iter_errors(value).map(format_validation_issue).collect::<Vec<_>>();

        if validation_issues.is_empty() {
            return Ok(());
        }

        validation_issues.sort();
        validation_issues.dedup();

        Err(target.mismatch_message(&validation_issues))
    }

    fn is_finalize_success_with_output(&self, arguments: &Value) -> bool {
        matches!(self.source, ModelToolSource::Finalize)
            && arguments
                .get("type")
                .and_then(Value::as_str)
                .and_then(FinalizeCallKind::from_identifier)
                == Some(FinalizeCallKind::Success)
            && arguments.get("output").is_some()
    }

    fn parse_finalize_arguments(&self, arguments: Value) -> Result<FinalizeResult, ProviderError> {
        match arguments
            .get("type")
            .and_then(Value::as_str)
            .and_then(FinalizeCallKind::from_identifier)
        {
            Some(FinalizeCallKind::Success) => Ok(FinalizeResult::Success(arguments.get("output").cloned().unwrap_or(Value::Null))),
            Some(FinalizeCallKind::Fail) => Ok(FinalizeResult::Fail),
            _ => Err(ProviderError::model(
                "unknown".to_string(),
                "validated finalize arguments did not include a supported type".to_string(),
            )),
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
    ) -> Result<ToolCallOutcome, ProviderError> {
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
            ),
            ModelToolSource::Finalize => unreachable!("finalize tool calls should return before MCP dispatch"),
            ModelToolSource::Local => Err(ProviderError::model(
                request.agent_name.clone(),
                format!("tool `{}` is not backed by MCP", self.name),
            )),
        }
    }

    fn execute_mcp_tool(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpToolTarget<'_>,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<ToolCallOutcome, ProviderError> {
        let server_config = McpServerConfig {
            name: target.server_name.unwrap_or("default").to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::Call,
            self.name.clone(),
            server_config.name.clone(),
            target.tool_name.to_string(),
            &arguments,
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
            Err(McpError::ToolCallFailed {
                server_name,
                tool_name,
                message: _,
                detail: _,
            }) => {
                let tool_error = json!({
                    "error": ModelToolErrorCode::McpCallFailed.as_str(),
                    "server_name": server_name,
                    "tool_name": tool_name,
                    "message": "MCP tool reported a failure",
                });

                request.send_mcp_call_failed(call_details, started_at.elapsed());

                return Ok(ToolCallOutcome::Continue(tool_error));
            }
            Err(error) => {
                request.send_mcp_call_failed(call_details, started_at.elapsed());

                return Err(ProviderError::mcp_with_source(
                    request.agent_name.clone(),
                    server_config.name.clone(),
                    target.tool_name.to_string(),
                    "MCP tool request failed",
                    error,
                ));
            }
        };
        let normalized_result = normalize_mcp_tool_result(result);
        let output_schema = self.output_schema.json_value_with_cache(schema_cache);

        if let Err(message) = self.validate_value(&normalized_result, &output_schema, ModelToolValidationTarget::Output) {
            let output_error = json!({
                "error": "tool_output_schema_mismatch",
                "tool_name": self.name,
                "message": message,
                "expected_schema": output_schema,
                "output": normalized_result,
            });

            request.send_mcp_call_failed(call_details, started_at.elapsed());

            return Ok(ToolCallOutcome::Continue(output_error));
        }

        let projected_result = self.output_schema.project_json_value(&normalized_result);

        request.send_mcp_call_completed(call_details, &projected_result, started_at.elapsed());
        log::debug!("completed MCP tool call: agent={}, tool={}", request.agent_name, self.name);

        Ok(ToolCallOutcome::Continue(projected_result))
    }

    fn execute_mcp_prompt(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
    ) -> Result<ToolCallOutcome, ProviderError> {
        let server_config = McpServerConfig {
            name: target.server_name.to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::Render,
            self.name.clone(),
            server_config.name.clone(),
            target.item_name.to_string(),
            &arguments,
        );

        request.send_mcp_call_started(&call_details);
        let started_at = Instant::now();
        let result = match request.mcp_pool.get(&server_config)?.get_prompt(target.item_name, arguments) {
            Ok(result) => result,
            Err(error) => {
                request.send_mcp_call_failed(call_details, started_at.elapsed());

                return Err(ProviderError::mcp_with_source(
                    request.agent_name.clone(),
                    server_config.name.clone(),
                    target.item_name.to_string(),
                    "MCP prompt request failed",
                    error,
                ));
            }
        };
        let rendered_result = Value::String(render_mcp_prompt_text_result(&result));

        request.send_mcp_call_completed(call_details, &rendered_result, started_at.elapsed());

        Ok(ToolCallOutcome::Continue(rendered_result))
    }

    fn execute_mcp_resource(
        &self,
        request: &ModelRequest,
        arguments: Value,
        target: McpImportTarget<'_>,
    ) -> Result<ToolCallOutcome, ProviderError> {
        let server_config = McpServerConfig {
            name: target.server_name.to_string(),
            endpoint: target.endpoint.to_string(),
            headers: target.headers.clone(),
        };
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::Read,
            self.name.clone(),
            server_config.name.clone(),
            target.item_name.to_string(),
            &arguments,
        );

        request.send_mcp_call_started(&call_details);
        let started_at = Instant::now();
        let result = match request.mcp_pool.get(&server_config)?.read_resource(target.item_name, arguments) {
            Ok(result) => result,
            Err(error) => {
                request.send_mcp_call_failed(call_details, started_at.elapsed());

                return Err(ProviderError::mcp_with_source(
                    request.agent_name.clone(),
                    server_config.name.clone(),
                    target.item_name.to_string(),
                    "MCP resource request failed",
                    error,
                ));
            }
        };
        let rendered_result = Value::String(render_mcp_resource_text_result(&result));

        request.send_mcp_call_completed(call_details, &rendered_result, started_at.elapsed());

        Ok(ToolCallOutcome::Continue(rendered_result))
    }
}

trait ModelSchemaChatCompletionOutputExt {
    fn parse_chat_completion_text_output(
        &self,
        response_text: &str,
        agent_name: &str,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<Value, ProviderError>;

    fn validate_chat_completion_output(
        &self,
        output: Value,
        agent_name: &str,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<Value, ProviderError>;

    fn single_required_string_property_name(&self, schema_cache: &mut ModelSchemaCache) -> Option<String>;
}

impl ModelSchemaChatCompletionOutputExt for ModelSchema {
    fn parse_chat_completion_text_output(
        &self,
        response_text: &str,
        agent_name: &str,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<Value, ProviderError> {
        if let Some(parsed_output) = response_text.parse_json_response_value() {
            if let Ok(output) = self.validate_chat_completion_output(parsed_output, agent_name, schema_cache) {
                return Ok(output);
            }
        }

        if self.schema_type_name_with_cache(schema_cache).as_deref() == Some("string") {
            return self.validate_chat_completion_output(Value::String(response_text.trim().to_string()), agent_name, schema_cache);
        }

        if let Some(property_name) = self.single_required_string_property_name(schema_cache) {
            let mut output_object = serde_json::Map::new();

            output_object.insert(property_name, Value::String(response_text.trim().to_string()));

            return self.validate_chat_completion_output(Value::Object(output_object), agent_name, schema_cache);
        }

        Err(ProviderError::model(
            agent_name.to_string(),
            "file response did not match the declared output schema".to_string(),
        ))
    }

    fn validate_chat_completion_output(
        &self,
        output: Value,
        agent_name: &str,
        schema_cache: &mut ModelSchemaCache,
    ) -> Result<Value, ProviderError> {
        let output_schema = self.json_value_with_cache(schema_cache);
        let validator = jsonschema::validator_for(&output_schema).map_err(|error| {
            ProviderError::model_with_source(agent_name.to_string(), "model output schema could not be compiled", error)
        })?;

        if validator.is_valid(&output) {
            return Ok(self.project_json_value(&output));
        }

        Err(ProviderError::model(
            agent_name.to_string(),
            "file response did not match the declared output schema",
        ))
    }

    fn single_required_string_property_name(&self, schema_cache: &mut ModelSchemaCache) -> Option<String> {
        let output_schema = self.json_value_with_cache(schema_cache);

        if output_schema.get("type").and_then(Value::as_str) != Some("object") {
            return None;
        }

        let properties = output_schema.get("properties").and_then(Value::as_object)?;

        if properties.len() != 1 {
            return None;
        }

        let (property_name, property_schema) = properties.iter().next()?;

        if property_schema.get("type").and_then(Value::as_str) != Some("string") {
            return None;
        }

        let required_fields = output_schema.get("required").and_then(Value::as_array)?;
        let property_is_required = required_fields
            .iter()
            .any(|required_field| required_field.as_str() == Some(property_name));

        property_is_required.then(|| property_name.clone())
    }
}

trait ResponseTextJsonExt {
    fn parse_json_response_value(&self) -> Option<Value>;
}

impl ResponseTextJsonExt for str {
    fn parse_json_response_value(&self) -> Option<Value> {
        let trimmed_text = self.trim();

        if let Ok(value) = serde_json::from_str::<Value>(trimmed_text) {
            return Some(value);
        }

        let fenced_text = trimmed_text.strip_prefix("```")?;
        let (_, fenced_body) = fenced_text.split_once('\n')?;
        let json_text = fenced_body.strip_suffix("```")?.trim();

        serde_json::from_str::<Value>(json_text).ok()
    }
}

trait ProviderConfigCerseiExt {
    fn build_provider(
        &self,
        request: &ModelRequest,
        endpoint_approval: &network::ProviderEndpointApproval,
    ) -> Result<Box<dyn Provider>, ProviderError>;
    fn resolved_endpoint(&self) -> Option<String>;

    fn resolved_api_key(&self) -> Option<String>;

    fn has_custom_endpoint(&self) -> bool;

    fn uses_builtin_endpoint(&self) -> bool;

    fn required_api_key(&self, request: &ModelRequest, api_key: Option<String>) -> Result<String, ProviderError>;
}

impl ProviderConfigCerseiExt for ProviderConfig {
    fn build_provider(
        &self,
        request: &ModelRequest,
        endpoint_approval: &network::ProviderEndpointApproval,
    ) -> Result<Box<dyn Provider>, ProviderError> {
        let endpoint = endpoint_approval.endpoint();
        let client = endpoint_approval.http_client();
        let api_key = self.resolved_api_key();

        log::debug!(
            "building Cersei provider: agent={}, provider={}, custom_endpoint={}, api_key={}",
            request.agent_name,
            self.driver.as_str(),
            self.has_custom_endpoint(),
            if api_key.is_some() { "configured" } else { "missing" }
        );

        match self.driver.api_format() {
            ProviderApiFormat::Anthropic => {
                let api_key = self.required_api_key(request, api_key)?;
                let builder = Anthropic::builder()
                    .api_key(api_key)
                    .base_url(endpoint)
                    .model(request.model_name.clone())
                    .client(client);

                builder
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| {
                        ProviderError::model_with_source(request.agent_name.clone(), "failed to build Anthropic provider", error)
                    })
            }
            ProviderApiFormat::Google => {
                let api_key = self.required_api_key(request, api_key)?;
                let builder = Gemini::builder()
                    .api_key(api_key)
                    .base_url(endpoint)
                    .model(request.model_name.clone())
                    .client(client);

                builder
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| ProviderError::model_with_source(request.agent_name.clone(), "failed to build Gemini provider", error))
            }
            ProviderApiFormat::OpenAiCompatible => {
                let api_key = if self.driver == ProviderDriver::Ollama {
                    api_key.unwrap_or_else(|| "no-key".to_string())
                } else {
                    self.required_api_key(request, api_key)?
                };

                OpenAi::builder()
                    .base_url(endpoint)
                    .api_key(api_key)
                    .model(request.model_name.clone())
                    .client(client)
                    .build()
                    .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    .map_err(|error| {
                        ProviderError::model_with_source(request.agent_name.clone(), "failed to build OpenAI-compatible provider", error)
                    })
            }
        }
    }

    fn resolved_endpoint(&self) -> Option<String> {
        self.endpoint.clone().or_else(|| self.driver.default_endpoint().map(str::to_string))
    }

    fn resolved_api_key(&self) -> Option<String> {
        self.api_key.clone().or_else(|| {
            self.uses_builtin_endpoint()
                .then(|| self.driver.api_key_from_environment())
                .flatten()
        })
    }

    fn has_custom_endpoint(&self) -> bool {
        self.endpoint
            .as_deref()
            .is_some_and(|endpoint| Some(endpoint) != self.driver.default_endpoint())
    }

    fn uses_builtin_endpoint(&self) -> bool {
        let Some(default_endpoint) = self.driver.default_endpoint() else {
            return false;
        };

        match self.endpoint.as_deref() {
            Some(endpoint) => endpoint == default_endpoint,
            None => true,
        }
    }

    fn required_api_key(&self, request: &ModelRequest, api_key: Option<String>) -> Result<String, ProviderError> {
        if !self.driver.requires_api_key() {
            return Ok(api_key.unwrap_or_else(|| "no-key".to_string()));
        }

        api_key.ok_or_else(|| {
            let message = if self.has_custom_endpoint() {
                format!(
                    "provider `{}` requires an explicit `api_key` for a custom endpoint",
                    self.driver.as_str()
                )
            } else if self.driver.api_key_environment_variables().is_empty() {
                format!("provider `{}` requires an explicit `api_key`", self.driver.as_str())
            } else {
                format!(
                    "provider `{}` requires `api_key` or one of these environment variables: {}",
                    self.driver.as_str(),
                    self.driver.api_key_environment_variables().join(", ")
                )
            };

            ProviderError::model(request.agent_name.clone(), message)
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
    fn send_agent_file_created(&self, uploaded_file: &UploadedProviderFile);

    fn send_agent_file_deleted(&self, uploaded_file: &UploadedProviderFile);

    fn send_tool_call_failed(&self, tool_name: &str, duration: Duration);

    fn send_mcp_tool_validation_started(&self, tool_name: &str, arguments: &Value);

    fn send_mcp_tool_validation_failed(&self, tool_name: &str, duration: Duration);

    fn send_mcp_tool_validation_completed(&self, tool_name: &str, duration: Duration);

    fn send_mcp_call_started(&self, details: &McpCallEventDetails);

    fn send_mcp_call_failed(&self, details: McpCallEventDetails, duration: Duration);

    fn send_mcp_call_completed(&self, details: McpCallEventDetails, result: &Value, duration: Duration);
}

impl ToolCallEventSender for ModelRequest {
    fn send_agent_file_created(&self, uploaded_file: &UploadedProviderFile) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::agent_file_created(
                self.agent_name.clone(),
                uploaded_file.filename.clone(),
                uploaded_file.purpose.clone(),
                uploaded_file.bytes,
            ));
        }
    }

    fn send_agent_file_deleted(&self, uploaded_file: &UploadedProviderFile) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::agent_file_deleted(
                self.agent_name.clone(),
                uploaded_file.filename.clone(),
                uploaded_file.purpose.clone(),
            ));
        }
    }

    fn send_tool_call_failed(&self, tool_name: &str, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::tool_call_failed(
                self.agent_name.clone(),
                tool_name.to_string(),
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_started(&self, tool_name: &str, arguments: &Value) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::mcp_tool_validation_started(
                self.agent_name.clone(),
                tool_name.to_string(),
                arguments,
            ));
        }
    }

    fn send_mcp_tool_validation_failed(&self, tool_name: &str, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::mcp_tool_validation_failed(
                self.agent_name.clone(),
                tool_name.to_string(),
                duration,
            ));
        }
    }

    fn send_mcp_tool_validation_completed(&self, tool_name: &str, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::mcp_tool_validation_completed(
                self.agent_name.clone(),
                tool_name.to_string(),
                duration,
            ));
        }
    }

    fn send_mcp_call_started(&self, details: &McpCallEventDetails) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::mcp_call_started(details.clone()).with_agent_name(self.agent_name.clone()));
        }
    }

    fn send_mcp_call_failed(&self, details: McpCallEventDetails, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            event_sender.try_send_observed(ExecutorEvent::mcp_call_failed(details, duration).with_agent_name(self.agent_name.clone()));
        }
    }

    fn send_mcp_call_completed(&self, details: McpCallEventDetails, result: &Value, duration: Duration) {
        if let Some(event_sender) = &self.event_sender {
            event_sender
                .try_send_observed(ExecutorEvent::mcp_call_completed(details, result, duration).with_agent_name(self.agent_name.clone()));
        }
    }
}

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use superwire_model::ToolCallLimitScope;

    #[test]
    fn validates_discovered_mcp_invocation_boundaries() {
        let tool_definition = ModelToolDefinition {
            name: "validate_label".to_string(),
            description: None,
            source: ModelToolSource::Local,
            input_schema: ModelSchema::OpenObject,
            output_schema: ModelSchema::OpenObject,
            invocation_schema: Some(ModelSchema::json(json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "minLength": 1
                    }
                },
                "required": ["label"],
                "additionalProperties": false
            }))),
            bindings: json!({ "label": "" }),
            max_calls: None,
            max_calls_scope: ToolCallLimitScope::Workflow,
        };
        let mut arguments = json!({});
        let mut schema_cache = ModelSchemaCache::default();

        tool_definition.merge_bindings_into(&mut arguments);
        let invocation_schema = tool_definition
            .invocation_schema
            .as_ref()
            .expect("invocation schema should exist")
            .json_value_with_cache(&mut schema_cache);
        let validation_result = tool_definition.validate_value(&arguments, &invocation_schema, ModelToolValidationTarget::Arguments);

        assert!(validation_result.is_err());

        let output_schema = json!({
            "type": "object",
            "properties": {
                "accepted": { "type": "boolean" }
            },
            "required": ["accepted"],
            "additionalProperties": false
        });
        let output_validation_result =
            tool_definition.validate_value(&json!({ "accepted": "yes" }), &output_schema, ModelToolValidationTarget::Output);

        assert!(output_validation_result.is_err());
    }
}
