#![allow(dead_code)]

use crate::model::{ModelProvider, ModelProviderError, ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use crate::service::ExecutorService;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use superwire_mcp::{HttpMcpClientFactory, McpNetworkPolicy};
use superwire_protocol::api::{ExecutionOptions, ExecutionRequest};

// ---------------------------------------------------------------------------
// Mock providers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TestModelProvider {
    outputs: Arc<Mutex<VecDeque<Value>>>,
}

impl TestModelProvider {
    pub fn new(outputs: Vec<Value>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(VecDeque::from(outputs))),
        }
    }
}

#[async_trait]
impl ModelProvider for TestModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        let output = self
            .outputs
            .lock()
            .expect("test runner outputs lock should not be poisoned")
            .pop_front()
            .unwrap_or_else(|| serde_json::json!(request.agent_name));

        Ok(ModelResponse {
            output,
            context: serde_json::json!({ "agent": request.agent_name }),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PanickingModelProvider;

#[async_trait]
impl ModelProvider for PanickingModelProvider {
    async fn generate(&self, _model_request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        panic!("scripted model provider panic");
    }
}

#[derive(Debug, Clone)]
pub struct ScriptedModelProvider {
    responses: Arc<Mutex<HashMap<String, VecDeque<Value>>>>,
    default_output: Option<Value>,
}

impl ScriptedModelProvider {
    pub fn new(responses: HashMap<String, Vec<Value>>) -> Self {
        let mapped = responses.into_iter().map(|(key, values)| (key, VecDeque::from(values))).collect();

        Self {
            responses: Arc::new(Mutex::new(mapped)),
            default_output: None,
        }
    }

    pub fn with_default(mut self, default: Value) -> Self {
        self.default_output = Some(default);
        self
    }
}

#[async_trait]
impl ModelProvider for ScriptedModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        let mut responses = self.responses.lock().expect("scripted provider lock should not be poisoned");

        let output = if let Some(queue) = responses.get_mut(&request.agent_name) {
            queue.pop_front().or_else(|| self.default_output.clone())
        } else {
            self.default_output.clone()
        };

        let output = output.unwrap_or_else(|| serde_json::json!(request.agent_name));

        Ok(ModelResponse {
            output,
            context: serde_json::json!({ "agent": request.agent_name }),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TrackingModelProvider {
    inner: TestModelProvider,
    pub recorded_requests: Arc<Mutex<Vec<ModelRequest>>>,
}

impl TrackingModelProvider {
    pub fn new(outputs: Vec<Value>) -> Self {
        Self {
            inner: TestModelProvider::new(outputs),
            recorded_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn recorded_agent_names(&self) -> Vec<String> {
        self.recorded_requests
            .lock()
            .expect("tracking lock should not be poisoned")
            .iter()
            .map(|request| request.agent_name.clone())
            .collect()
    }

    pub fn recorded_prompts(&self) -> Vec<String> {
        self.recorded_requests
            .lock()
            .expect("tracking lock should not be poisoned")
            .iter()
            .map(|request| request.prompt.clone())
            .collect()
    }

    pub fn recorded_count(&self) -> usize {
        self.recorded_requests.lock().expect("tracking lock should not be poisoned").len()
    }
}

#[async_trait]
impl ModelProvider for TrackingModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        self.recorded_requests
            .lock()
            .expect("tracking lock should not be poisoned")
            .push(ModelRequest {
                agent_name: request.agent_name.clone(),
                provider_config: request.provider_config.clone(),
                model_name: request.model_name.clone(),
                wire_api: request.wire_api,
                inference: request.inference.clone(),
                context: request.context.clone(),
                prompt: request.prompt.clone(),
                prompt_content: request.prompt_content.clone(),
                file_attachments: request.file_attachments.clone(),
                output_schema: request.output_schema.clone(),
                tools: request.tools.clone(),
                event_sender: request.event_sender.clone(),
                mcp_pool: request.mcp_pool.clone(),
                tool_call_tracker: request.tool_call_tracker.clone(),
            });

        self.inner.generate(request).await
    }
}

#[derive(Debug, Clone)]
pub struct FailingModelProvider {
    message: String,
}

impl FailingModelProvider {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

#[async_trait]
impl ModelProvider for FailingModelProvider {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        Err(ModelProviderError::model("failing-provider".to_string(), self.message.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct RetryModelProvider {
    failures_before_success: usize,
    failure_message: String,
    call_count: Arc<AtomicUsize>,
}

impl RetryModelProvider {
    pub fn new(failures_before_success: usize, failure_message: impl Into<String>) -> Self {
        Self {
            failures_before_success,
            failure_message: failure_message.into(),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelProvider for RetryModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        let attempt = self.call_count.fetch_add(1, Ordering::SeqCst);

        if attempt < self.failures_before_success {
            return Err(ModelProviderError::model(
                request.agent_name,
                format!("{} (attempt {})", self.failure_message, attempt + 1),
            ));
        }

        Ok(ModelResponse {
            output: serde_json::json!({ "success": true }),
            context: serde_json::json!({ "agent": request.agent_name }),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ConcurrentTrackingModelProvider {
    active_requests: Arc<AtomicUsize>,
    max_active_requests: Arc<AtomicUsize>,
    response_delay: Duration,
}

impl ConcurrentTrackingModelProvider {
    pub fn new(response_delay: Duration) -> Self {
        Self {
            active_requests: Arc::new(AtomicUsize::new(0)),
            max_active_requests: Arc::new(AtomicUsize::new(0)),
            response_delay,
        }
    }

    pub fn max_active_requests(&self) -> usize {
        self.max_active_requests.load(Ordering::SeqCst)
    }

    pub fn active_requests(&self) -> usize {
        self.active_requests.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelProvider for ConcurrentTrackingModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        let active_request_guard = ActiveRequestGuard::new(self.active_requests.clone());
        let active_request_count = active_request_guard.active_request_count();

        self.max_active_requests.fetch_max(active_request_count, Ordering::SeqCst);
        tokio::time::sleep(self.response_delay).await;

        Ok(ModelResponse {
            output: serde_json::json!({ "value": request.agent_name }),
            context: serde_json::json!({ "agent": request.agent_name }),
        })
    }
}

struct ActiveRequestGuard {
    active_requests: Arc<AtomicUsize>,
    active_request_count: usize,
}

impl ActiveRequestGuard {
    fn new(active_requests: Arc<AtomicUsize>) -> Self {
        let active_request_count = active_requests.fetch_add(1, Ordering::SeqCst) + 1;

        Self {
            active_requests,
            active_request_count,
        }
    }

    fn active_request_count(&self) -> usize {
        self.active_request_count
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.active_requests.fetch_sub(1, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

pub fn service(outputs: Vec<Value>) -> ExecutorService<TestModelProvider> {
    ExecutorService::new(TestModelProvider::new(outputs))
}

pub fn service_with_trusted_mcp<ModelProviderType>(model_provider: ModelProviderType) -> ExecutorService<ModelProviderType> {
    ExecutorService::new(model_provider)
        .with_mcp_client_factory(Arc::new(HttpMcpClientFactory::for_network_policy(McpNetworkPolicy::Trusted)))
}

pub fn request(fixture: &str) -> ExecutionRequest {
    ExecutionRequest {
        workflow_source: Some(fixture.to_string()),
        workflow_source_base64: None,
        input: Value::Null,
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    }
}

pub fn request_with_input(fixture: &str, input: Value) -> ExecutionRequest {
    ExecutionRequest {
        workflow_source: Some(fixture.to_string()),
        workflow_source_base64: None,
        input,
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    }
}

pub fn request_with_secrets(fixture: &str, input: Value, secrets: Value) -> ExecutionRequest {
    ExecutionRequest {
        workflow_source: Some(fixture.to_string()),
        workflow_source_base64: None,
        input,
        secrets,
        options: ExecutionOptions::default(),
    }
}

pub async fn execute(fixture: &str, outputs: Vec<Value>) -> Value {
    service(outputs)
        .execute(request(fixture))
        .await
        .expect("execution should succeed")
        .output
}

pub async fn execute_with_input(fixture: &str, outputs: Vec<Value>, input: Value) -> Value {
    service(outputs)
        .execute(request_with_input(fixture, input))
        .await
        .expect("execution should succeed")
        .output
}

pub async fn execute_with_secrets(fixture: &str, outputs: Vec<Value>, input: Value, secrets: Value) -> Value {
    service(outputs)
        .execute(request_with_secrets(fixture, input, secrets))
        .await
        .expect("execution should succeed")
        .output
}

pub async fn execute_expect_error(fixture: &str, outputs: Vec<Value>) -> ExecutorError {
    service(outputs).execute(request(fixture)).await.expect_err("execution should fail")
}

pub async fn execute_with_input_expect_error(fixture: &str, outputs: Vec<Value>, input: Value) -> ExecutorError {
    service(outputs)
        .execute(request_with_input(fixture, input))
        .await
        .expect_err("execution should fail")
}

pub async fn execute_with_secrets_expect_error(fixture: &str, outputs: Vec<Value>, input: Value, secrets: Value) -> ExecutorError {
    service(outputs)
        .execute(request_with_secrets(fixture, input, secrets))
        .await
        .expect_err("execution should fail")
}
