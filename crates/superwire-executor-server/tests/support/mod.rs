#![allow(dead_code)]

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use superwire_executor::model::{ModelProvider, ModelProviderError, ModelRequest, ModelResponse};
use superwire_executor::ExecutorService;
use superwire_protocol::api::{ExecutionOptions, ExecutionRequest};

pub use superwire_test_support::fixtures;

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
            .unwrap_or_else(|| json!(request.agent_name));

        Ok(ModelResponse {
            output,
            context: json!({ "agent": request.agent_name }),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TrackingModelProvider {
    inner: TestModelProvider,
    recorded_requests: Arc<Mutex<Vec<ModelRequest>>>,
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
            .push(request.clone());

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

#[derive(Debug, Clone, Copy)]
pub struct PanickingModelProvider;

#[async_trait]
impl ModelProvider for PanickingModelProvider {
    async fn generate(&self, _model_request: ModelRequest) -> Result<ModelResponse, ModelProviderError> {
        panic!("scripted server model panic");
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
            output: json!({ "value": request.agent_name }),
            context: json!({ "agent": request.agent_name }),
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

pub fn service(outputs: Vec<Value>) -> ExecutorService<TestModelProvider> {
    ExecutorService::new(TestModelProvider::new(outputs))
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

pub async fn execute(fixture: &str, outputs: Vec<Value>) -> Value {
    service(outputs)
        .execute(request(fixture))
        .await
        .expect("execution should succeed")
        .output
}

pub struct TestMcpHttpServer {
    endpoint: String,
    request_count: Arc<AtomicUsize>,
}

#[derive(Clone, Copy, Debug)]
enum TestMcpMethod {
    Initialized,
    ToolsList,
    ToolsCall,
    Unknown,
}

#[derive(Clone, Copy)]
enum JsonSchemaType {
    String,
    Number,
    Boolean,
    Object,
}

struct SchemaField {
    name: &'static str,
    schema: Value,
}

struct TestMcpCatalog;

impl TestMcpHttpServer {
    pub fn spawn(expected_headers: impl IntoIterator<Item = (String, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let expected_headers = expected_headers.into_iter().collect::<BTreeMap<_, _>>();
        let request_count = Arc::new(AtomicUsize::new(0));
        let thread_request_count = Arc::clone(&request_count);

        thread::spawn(move || {
            let catalog = TestMcpCatalog;

            for incoming_stream in listener.incoming().take(12) {
                let stream = incoming_stream.expect("test MCP stream should open");
                thread_request_count.fetch_add(1, Ordering::Relaxed);
                catalog.handle_request(stream, &expected_headers);
            }
        });

        Self { endpoint, request_count }
    }

    pub fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    pub fn request_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
}

impl TestMcpMethod {
    fn from_request(request: &Value) -> Self {
        match request.get("method").and_then(Value::as_str) {
            Some("notifications/initialized") => Self::Initialized,
            Some("tools/list") => Self::ToolsList,
            Some("tools/call") => Self::ToolsCall,
            _ => Self::Unknown,
        }
    }
}

impl JsonSchemaType {
    fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Object => "object",
        }
    }
}

impl TestMcpCatalog {
    fn response_for(&self, method: TestMcpMethod, request: &Value) -> Option<Value> {
        match method {
            TestMcpMethod::Initialized => None,
            TestMcpMethod::ToolsList => Some(Self::jsonrpc_result(2, json!({ "tools": self.tools() }))),
            TestMcpMethod::ToolsCall => Some(Self::jsonrpc_result(3, self.tool_call_result(request))),
            TestMcpMethod::Unknown => Some(Self::jsonrpc_result(1, json!({}))),
        }
    }

    fn tool_call_result(&self, _request: &Value) -> Value {
        json!({ "content": [{ "type": "text", "text": "{}" }] })
    }

    fn tools(&self) -> Vec<Value> {
        vec![Self::mcp_tool(
            "update_user_name",
            "Update a user name",
            Self::object_schema(
                [
                    SchemaField::new("user_id", JsonSchemaType::Number.primitive_schema()),
                    SchemaField::new("user_name", JsonSchemaType::String.string_enum_schema(["Ada", "Grace"])),
                ],
                ["user_id", "user_name"],
            ),
            Self::object_schema(
                [SchemaField::new("success", JsonSchemaType::Boolean.primitive_schema())],
                ["success"],
            ),
        )]
    }

    fn handle_request(&self, mut stream: TcpStream, expected_headers: &BTreeMap<String, String>) {
        let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
        let mut request_headers = BTreeMap::new();
        let mut content_length = 0_usize;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            reader.read_line(&mut header_line).expect("header line should read");

            if header_line == "\r\n" || header_line.is_empty() {
                break;
            }

            if let Some(header_value) = header_line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = header_value.trim().parse().expect("content length should parse");
            }

            if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
                request_headers.insert(header_name.to_ascii_lowercase(), header_value.trim().to_string());
            }
        }

        for (header_name, header_value) in expected_headers {
            assert_eq!(
                request_headers.get(header_name),
                Some(header_value),
                "expected MCP request header `{header_name}`"
            );
        }

        let mut request_body = vec![0_u8; content_length];
        reader.read_exact(&mut request_body).expect("request body should read");
        let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");
        let method = TestMcpMethod::from_request(&request);

        let response = if let Some(response_body) = self.response_for(method, &request) {
            let response_body = response_body.to_string();

            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            )
        } else {
            "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\n\r\n".to_string()
        };

        stream.write_all(response.as_bytes()).expect("response should write");
    }

    fn jsonrpc_result(request_identifier: u64, result: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": request_identifier,
            "result": result
        })
    }

    fn mcp_tool(name: &str, description: &str, input_schema: Value, output_schema: Value) -> Value {
        json!({
            "name": name,
            "description": description,
            "inputSchema": input_schema,
            "outputSchema": output_schema
        })
    }

    fn object_schema(fields: impl IntoIterator<Item = SchemaField>, required_fields: impl IntoIterator<Item = &'static str>) -> Value {
        let properties = fields
            .into_iter()
            .map(|field| (field.name.to_string(), field.schema))
            .collect::<serde_json::Map<_, _>>();
        let required_fields = required_fields.into_iter().collect::<Vec<_>>();

        json!({
            "type": JsonSchemaType::Object.as_str(),
            "properties": properties,
            "required": required_fields
        })
    }
}

impl SchemaField {
    fn new(name: &'static str, schema: Value) -> Self {
        Self { name, schema }
    }
}

impl JsonSchemaType {
    fn primitive_schema(self) -> Value {
        json!({ "type": self.as_str() })
    }

    fn string_enum_schema(self, values: impl IntoIterator<Item = &'static str>) -> Value {
        json!({
            "type": self.as_str(),
            "enum": values.into_iter().collect::<Vec<_>>()
        })
    }
}
