use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_executor::runtime::{ExecutorError, WorkflowExecutor};
use superwire_provider_cersei::CerseiModelProvider;
pub use superwire_test_support::FakeMcpRequest;
use superwire_test_support::{fixtures, FakeMcpClientFactory, WorkflowSource};

type MessageAssertion = Arc<dyn Fn(&[Value]) + Send + Sync>;

#[derive(Debug)]
pub struct TestRunOutput {
    pub output: Value,
    pub provider_requests: BTreeMap<String, Vec<Value>>,
    pub mcp_requests: BTreeMap<String, Vec<FakeMcpRequest>>,
}

#[derive(Debug)]
pub struct TestRunErrorOutput {
    pub error: ExecutorError,
    pub provider_requests: BTreeMap<String, Vec<Value>>,
    pub mcp_requests: BTreeMap<String, Vec<FakeMcpRequest>>,
}

pub struct TestRunner {
    workflow_source: WorkflowSource,
    input: Value,
    secrets: Value,
    providers: BTreeMap<String, ProviderScript>,
    mcp_servers: BTreeMap<String, McpScript>,
    max_concurrency: usize,
}

#[derive(Default)]
pub struct ProviderBuilder {
    api_key: Option<String>,
    models: BTreeMap<String, ModelScript>,
}

#[derive(Default)]
pub struct ModelBuilder {
    turns: Vec<ModelTurn>,
}

#[derive(Default)]
struct ModelTurnDraft {
    expected_prompt: Option<String>,
    expected_tools: Option<Vec<String>>,
    expected_tool_schemas: BTreeMap<String, Value>,
    message_assertions: Vec<MessageAssertion>,
    response: Option<ModelTurnResponse>,
}

#[derive(Debug, Default)]
pub struct McpBuilder {
    tools: BTreeMap<String, McpToolScript>,
    resources: BTreeMap<String, McpResourceScript>,
    prompts: BTreeMap<String, McpPromptScript>,
}

#[derive(Debug, Default)]
pub struct McpToolBuilder {
    description: Option<String>,
    input_schema: Option<Value>,
    output_schema: Option<Value>,
    responses: VecDeque<Value>,
}

#[derive(Debug, Default)]
pub struct McpResourceBuilder {
    uri: Option<String>,
    mime_type: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Default)]
pub struct McpPromptBuilder {
    description: Option<String>,
    text: Option<String>,
    arguments: Vec<McpPromptArgumentScript>,
}

struct ProviderScript {
    api_key: String,
    models: BTreeMap<String, ModelScript>,
}

struct ModelScript {
    turns: VecDeque<ModelTurn>,
}

struct ModelTurn {
    expected_prompt: Option<String>,
    expected_tools: Option<Vec<String>>,
    expected_tool_schemas: BTreeMap<String, Value>,
    message_assertions: Vec<MessageAssertion>,
    response: ModelTurnResponse,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    name: String,
    arguments: Value,
}

#[derive(Debug, Clone)]
enum ModelTurnResponse {
    Json(Value),
    Text(String),
    ToolCalls(Vec<ToolCall>),
    Error(String),
}

#[derive(Debug)]
struct McpScript {
    tools: BTreeMap<String, McpToolScript>,
    resources: BTreeMap<String, McpResourceScript>,
    prompts: BTreeMap<String, McpPromptScript>,
}

#[derive(Debug)]
struct McpToolScript {
    description: String,
    input_schema: Value,
    output_schema: Value,
    responses: VecDeque<Value>,
}

#[derive(Debug)]
struct McpResourceScript {
    uri: String,
    mime_type: String,
    text: String,
}

#[derive(Debug)]
struct McpPromptScript {
    description: String,
    text: String,
    arguments: Vec<McpPromptArgumentScript>,
}

#[derive(Debug, Clone)]
struct McpPromptArgumentScript {
    name: String,
    description: String,
    required: bool,
}

struct ProviderServer {
    endpoint: String,
    state: Arc<Mutex<ProviderServerState>>,
}

struct ProviderServerState {
    models: BTreeMap<String, ModelScript>,
    requests: Vec<Value>,
    errors: Vec<String>,
    uploaded_files: Vec<String>,
    deleted_files: Vec<String>,
}

impl TestRunner {
    #[must_use]
    pub fn workflow(workflow_source: impl Into<String>) -> Self {
        Self {
            workflow_source: WorkflowSource::fixture_or_inline(fixtures::root(), workflow_source.into()),
            input: Value::Null,
            secrets: Value::Null,
            providers: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            max_concurrency: 1,
        }
    }

    #[must_use]
    pub fn input(mut self, input: Value) -> Self {
        self.input = input;
        self
    }

    #[must_use]
    pub fn secrets(mut self, secrets: Value) -> Self {
        self.secrets = secrets;
        self
    }

    #[must_use]
    pub fn max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency;
        self
    }

    #[must_use]
    pub fn provider(mut self, provider_name: impl Into<String>, configure: impl FnOnce(&mut ProviderBuilder)) -> Self {
        let mut builder = ProviderBuilder::default();
        configure(&mut builder);
        self.providers.insert(provider_name.into(), builder.build());
        self
    }

    #[must_use]
    pub fn mcp(mut self, server_name: impl Into<String>, configure: impl FnOnce(&mut McpBuilder)) -> Self {
        let mut builder = McpBuilder::default();
        configure(&mut builder);
        self.mcp_servers.insert(server_name.into(), builder.build());
        self
    }

    pub async fn run(self) -> Result<TestRunOutput, ExecutorError> {
        let provider_servers = self.spawn_provider_servers();
        let mcp_server_names = self.mcp_server_names();
        let mcp_client_factory = self.fake_mcp_client_factory();
        let workflow_source = self.workflow_source_with_mock_endpoints(&provider_servers)?;
        let executor = WorkflowExecutor::from_source_with_runtime_values_and_mcp_client_factory(
            &workflow_source,
            &self.input,
            &self.secrets,
            &mcp_client_factory,
        )?;
        let output = executor
            .execute(self.input, self.secrets, &CerseiModelProvider, None, self.max_concurrency)
            .await?;

        let provider_requests = verify_provider_servers(&provider_servers);
        let mcp_requests = collect_mcp_requests(&mcp_client_factory, &mcp_server_names);

        Ok(TestRunOutput {
            output,
            provider_requests,
            mcp_requests,
        })
    }

    pub async fn run_expect_error(self) -> TestRunErrorOutput {
        let provider_servers = self.spawn_provider_servers();
        let mcp_server_names = self.mcp_server_names();
        let mcp_client_factory = self.fake_mcp_client_factory();
        let workflow_source = self
            .workflow_source_with_mock_endpoints(&provider_servers)
            .expect("workflow source should be prepared");
        let execution_error = match WorkflowExecutor::from_source_with_runtime_values_and_mcp_client_factory(
            &workflow_source,
            &self.input,
            &self.secrets,
            &mcp_client_factory,
        ) {
            Ok(executor) => executor
                .execute(self.input, self.secrets, &CerseiModelProvider, None, self.max_concurrency)
                .await
                .expect_err("fixture runner should fail execution"),
            Err(error) => error,
        };

        let provider_requests = verify_provider_servers(&provider_servers);
        let mcp_requests = collect_mcp_requests(&mcp_client_factory, &mcp_server_names);

        TestRunErrorOutput {
            error: execution_error,
            provider_requests,
            mcp_requests,
        }
    }

    fn spawn_provider_servers(&self) -> BTreeMap<String, ProviderServer> {
        self.providers
            .iter()
            .map(|(provider_name, provider_script)| (provider_name.clone(), ProviderServer::spawn(provider_script)))
            .collect()
    }

    fn mcp_server_names(&self) -> Vec<String> {
        self.mcp_servers.keys().cloned().collect()
    }

    fn fake_mcp_client_factory(&self) -> FakeMcpClientFactory {
        let mut factory = FakeMcpClientFactory::new();

        for (server_name, script) in &self.mcp_servers {
            script.add_to_fake_factory(server_name, &mut factory);
        }

        factory
    }

    fn workflow_source_with_mock_endpoints(&self, provider_servers: &BTreeMap<String, ProviderServer>) -> Result<String, ExecutorError> {
        let mut workflow_source = self
            .workflow_source
            .read_formatted_or_original()
            .map_err(|error| ExecutorError::Other {
                message: error.to_string(),
            })?;

        for (provider_name, server) in provider_servers {
            workflow_source = replace_block_property(&workflow_source, "provider", provider_name, "endpoint", &json!(server.endpoint));
        }

        for server_name in self.mcp_servers.keys() {
            workflow_source = replace_block_property(
                &workflow_source,
                "mcp",
                server_name,
                "endpoint",
                &json!(format!("http://fake-mcp.test/{server_name}")),
            );
        }

        if !self.mcp_servers.is_empty() {
            workflow_source = workflow_source.replace("secrets {\n    mcp_endpoint: string\n}\n\n", "");
        }

        Ok(workflow_source)
    }
}

impl ProviderBuilder {
    pub fn api_key(&mut self, api_key: impl Into<String>) -> &mut Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn model(&mut self, model_name: impl Into<String>, configure: impl FnOnce(&mut ModelBuilder)) -> &mut Self {
        let mut builder = ModelBuilder::default();
        configure(&mut builder);
        self.models.insert(model_name.into(), builder.build());
        self
    }

    fn build(self) -> ProviderScript {
        ProviderScript {
            api_key: self.api_key.unwrap_or_else(|| "test-api-key".to_string()),
            models: self.models,
        }
    }
}

impl ModelBuilder {
    pub fn turn(&mut self) -> ModelTurnBuilder<'_> {
        ModelTurnBuilder::new(self)
    }

    fn push_turn(&mut self, turn: ModelTurn) {
        self.turns.push(turn);
    }

    fn build(self) -> ModelScript {
        ModelScript {
            turns: VecDeque::from(self.turns),
        }
    }
}

pub struct ModelTurnBuilder<'model> {
    model_builder: &'model mut ModelBuilder,
    turn: ModelTurnDraft,
}

impl<'model> ModelTurnBuilder<'model> {
    fn new(model_builder: &'model mut ModelBuilder) -> Self {
        Self {
            model_builder,
            turn: ModelTurnDraft::default(),
        }
    }

    pub fn with_messages(mut self, assertion: impl Fn(&[Value]) + Send + Sync + 'static) -> Self {
        self.turn.message_assertions.push(Arc::new(assertion));
        self
    }

    pub fn expect_prompt(mut self, expected_prompt: impl Into<String>) -> Self {
        self.turn.expected_prompt = Some(expected_prompt.into());
        self
    }

    pub fn expect_tools<const TOOL_COUNT: usize>(mut self, expected_tools: [&str; TOOL_COUNT]) -> Self {
        self.turn.expected_tools = Some(expected_tools.into_iter().map(str::to_string).collect());
        self
    }

    pub fn expect_tool_with_schema(mut self, tool_name: impl Into<String>, schema: Value) -> Self {
        self.turn.expected_tool_schemas.insert(tool_name.into(), schema);
        self
    }

    pub fn respond_json(mut self, response: Value) -> &'model mut ModelBuilder {
        self.turn.response = Some(ModelTurnResponse::Json(response));
        self.finish()
    }

    pub fn respond_string(mut self, response: impl Into<String>) -> &'model mut ModelBuilder {
        self.turn.response = Some(ModelTurnResponse::Json(Value::String(response.into())));
        self.finish()
    }

    pub fn respond_text(mut self, response: impl Into<String>) -> &'model mut ModelBuilder {
        self.turn.response = Some(ModelTurnResponse::Text(response.into()));
        self.finish()
    }

    pub fn respond_tool_calls<const CALL_COUNT: usize>(mut self, calls: [ToolCall; CALL_COUNT]) -> &'model mut ModelBuilder {
        self.turn.response = Some(ModelTurnResponse::ToolCalls(calls.into_iter().collect()));
        self.finish()
    }

    pub fn respond_error(mut self, message: impl Into<String>) -> &'model mut ModelBuilder {
        self.turn.response = Some(ModelTurnResponse::Error(message.into()));
        self.finish()
    }

    fn finish(self) -> &'model mut ModelBuilder {
        self.model_builder.push_turn(self.turn.build());
        self.model_builder
    }
}

impl ModelTurnDraft {
    fn build(self) -> ModelTurn {
        ModelTurn {
            expected_prompt: self.expected_prompt,
            expected_tools: self.expected_tools,
            expected_tool_schemas: self.expected_tool_schemas,
            message_assertions: self.message_assertions,
            response: self.response.expect("model turn must define a response"),
        }
    }
}

impl McpBuilder {
    pub fn tool(&mut self, tool_name: impl Into<String>, configure: impl FnOnce(&mut McpToolBuilder)) -> &mut Self {
        let mut builder = McpToolBuilder::default();
        configure(&mut builder);
        self.tools.insert(tool_name.into(), builder.build());
        self
    }

    pub fn resource(&mut self, resource_name: impl Into<String>, configure: impl FnOnce(&mut McpResourceBuilder)) -> &mut Self {
        let resource_name = resource_name.into();
        let mut builder = McpResourceBuilder::default();
        configure(&mut builder);
        self.resources.insert(resource_name.clone(), builder.build(resource_name));
        self
    }

    pub fn prompt(&mut self, prompt_name: impl Into<String>, configure: impl FnOnce(&mut McpPromptBuilder)) -> &mut Self {
        let prompt_name = prompt_name.into();
        let mut builder = McpPromptBuilder::default();
        configure(&mut builder);
        self.prompts.insert(prompt_name, builder.build());
        self
    }

    fn build(self) -> McpScript {
        McpScript {
            tools: self.tools,
            resources: self.resources,
            prompts: self.prompts,
        }
    }
}

impl McpToolBuilder {
    pub fn description(&mut self, description: impl Into<String>) -> &mut Self {
        self.description = Some(description.into());
        self
    }

    pub fn input_schema(&mut self, input_schema: Value) -> &mut Self {
        self.input_schema = Some(input_schema);
        self
    }

    pub fn output_schema(&mut self, output_schema: Value) -> &mut Self {
        self.output_schema = Some(output_schema);
        self
    }

    pub fn respond_json(&mut self, response: Value) -> &mut Self {
        self.responses.push_back(response);
        self
    }

    fn build(self) -> McpToolScript {
        McpToolScript {
            description: self.description.unwrap_or_else(|| "Test MCP tool".to_string()),
            input_schema: self.input_schema.unwrap_or_else(superwire_test_support::empty_object_schema),
            output_schema: self.output_schema.unwrap_or_else(superwire_test_support::empty_object_schema),
            responses: self.responses,
        }
    }
}

impl McpResourceBuilder {
    pub fn uri(&mut self, uri: impl Into<String>) -> &mut Self {
        self.uri = Some(uri.into());
        self
    }

    pub fn mime_type(&mut self, mime_type: impl Into<String>) -> &mut Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn text(&mut self, text: impl Into<String>) -> &mut Self {
        self.text = Some(text.into());
        self
    }

    fn build(self, resource_name: String) -> McpResourceScript {
        McpResourceScript {
            uri: self.uri.unwrap_or_else(|| format!("file://{resource_name}")),
            mime_type: self.mime_type.unwrap_or_else(|| "text/plain".to_string()),
            text: self.text.unwrap_or_default(),
        }
    }
}

impl McpPromptBuilder {
    pub fn description(&mut self, description: impl Into<String>) -> &mut Self {
        self.description = Some(description.into());
        self
    }

    pub fn text(&mut self, text: impl Into<String>) -> &mut Self {
        self.text = Some(text.into());
        self
    }

    pub fn argument(&mut self, name: impl Into<String>, required: bool) -> &mut Self {
        let name = name.into();

        self.arguments.push(McpPromptArgumentScript {
            description: format!("Test prompt argument {name}"),
            name,
            required,
        });

        self
    }

    fn build(self) -> McpPromptScript {
        McpPromptScript {
            description: self.description.unwrap_or_else(|| "Test MCP prompt".to_string()),
            text: self.text.unwrap_or_default(),
            arguments: self.arguments,
        }
    }
}

impl ToolCall {
    #[must_use]
    pub fn new(name: impl Into<String>, arguments: Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

impl Clone for McpScript {
    fn clone(&self) -> Self {
        Self {
            tools: self.tools.clone(),
            resources: self.resources.clone(),
            prompts: self.prompts.clone(),
        }
    }
}

impl Clone for McpToolScript {
    fn clone(&self) -> Self {
        Self {
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
            responses: self.responses.clone(),
        }
    }
}

impl Clone for McpResourceScript {
    fn clone(&self) -> Self {
        Self {
            uri: self.uri.clone(),
            mime_type: self.mime_type.clone(),
            text: self.text.clone(),
        }
    }
}

impl Clone for McpPromptScript {
    fn clone(&self) -> Self {
        Self {
            description: self.description.clone(),
            text: self.text.clone(),
            arguments: self.arguments.clone(),
        }
    }
}

impl McpScript {
    fn add_to_fake_factory(&self, server_name: &str, factory: &mut FakeMcpClientFactory) {
        factory.add_server(server_name, |server| {
            for (tool_name, tool) in &self.tools {
                server.tool(tool_name, |fake_tool| {
                    fake_tool
                        .description(tool.description.clone())
                        .input_schema(tool.input_schema.clone())
                        .output_schema(tool.output_schema.clone());

                    for response in &tool.responses {
                        fake_tool.respond_json(response.clone());
                    }
                });
            }

            for (resource_name, resource) in &self.resources {
                server.resource(resource_name, |fake_resource| {
                    fake_resource
                        .uri(resource.uri.clone())
                        .mime_type(resource.mime_type.clone())
                        .text(resource.text.clone());
                });
            }

            for (prompt_name, prompt) in &self.prompts {
                server.prompt(prompt_name, |fake_prompt| {
                    fake_prompt.description(prompt.description.clone()).text(prompt.text.clone());

                    for argument in &prompt.arguments {
                        fake_prompt.argument(argument.name.clone(), argument.required);
                    }
                });
            }
        });
    }
}

impl ProviderServer {
    fn spawn(script: &ProviderScript) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("provider test listener should bind");
        let endpoint = format!("http://{}/v1", listener.local_addr().expect("provider local address should exist"));
        let state = Arc::new(Mutex::new(ProviderServerState {
            models: script.models.clone(),
            requests: Vec::new(),
            errors: Vec::new(),
            uploaded_files: Vec::new(),
            deleted_files: Vec::new(),
        }));
        let thread_state = Arc::clone(&state);
        let expected_api_key = script.api_key.clone();

        thread::spawn(move || {
            for incoming_stream in listener.incoming() {
                let stream = incoming_stream.expect("provider test stream should open");
                handle_provider_request(stream, &thread_state, &expected_api_key);
            }
        });

        Self { endpoint, state }
    }
}

impl Clone for ModelScript {
    fn clone(&self) -> Self {
        Self { turns: self.turns.clone() }
    }
}

impl Clone for ModelTurn {
    fn clone(&self) -> Self {
        Self {
            expected_prompt: self.expected_prompt.clone(),
            expected_tools: self.expected_tools.clone(),
            expected_tool_schemas: self.expected_tool_schemas.clone(),
            message_assertions: self.message_assertions.clone(),
            response: self.response.clone(),
        }
    }
}

fn handle_provider_request(mut stream: TcpStream, state: &Arc<Mutex<ProviderServerState>>, expected_api_key: &str) {
    let Some(request) = read_http_request(&stream) else {
        return;
    };
    let response = build_provider_response(&request, state, expected_api_key).unwrap_or_else(|message| {
        state
            .lock()
            .expect("provider state lock should not be poisoned")
            .errors
            .push(message.clone());
        http_json_response(500, json!({ "error": { "message": message } }))
    });

    stream.write_all(response.as_bytes()).expect("provider response should write");
}

fn build_provider_response(
    request: &HttpRequest,
    state: &Arc<Mutex<ProviderServerState>>,
    expected_api_key: &str,
) -> Result<String, String> {
    let expected_authorization = format!("Bearer {expected_api_key}");

    if request.headers.get("authorization") != Some(&expected_authorization) {
        return Err(format!(
            "expected provider authorization `{expected_authorization}`, got {:?}",
            request.headers.get("authorization")
        ));
    }

    if request.method == "POST" && request.path == "/v1/files" {
        let mut state = state.lock().expect("provider state lock should not be poisoned");
        let file_id = format!("file-fe-test-{}", state.uploaded_files.len() + 1);

        state.uploaded_files.push(file_id.clone());

        return Ok(http_json_response(
            200,
            json!({
                "id": file_id,
                "bytes": request.body.len(),
                "created_at": 1_729_065_448,
                "filename": "uploaded.txt",
                "object": "file",
                "purpose": "file-extract",
                "status": "processed",
                "status_details": null
            }),
        ));
    }

    if request.method == "DELETE" {
        if let Some(file_id) = request.path.strip_prefix("/v1/files/") {
            state
                .lock()
                .expect("provider state lock should not be poisoned")
                .deleted_files
                .push(file_id.to_string());

            return Ok(http_json_response(
                200,
                json!({
                    "id": file_id,
                    "deleted": true,
                    "object": "file"
                }),
            ));
        }
    }

    if request.method != "POST" || request.path != "/v1/chat/completions" {
        return Err(format!("unexpected provider request {} {}", request.method, request.path));
    }

    let request_body = serde_json::from_slice::<Value>(&request.body).map_err(|error| format!("request body should be JSON: {error}"))?;
    let model_name = request_body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| "provider request missing model".to_string())?;
    let messages = request_body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "provider request missing messages".to_string())?;
    let mut state = state.lock().expect("provider state lock should not be poisoned");
    state.requests.push(request_body.clone());
    let model = state
        .models
        .get_mut(model_name)
        .ok_or_else(|| format!("unexpected provider model `{model_name}`"))?;
    let turn = model
        .turns
        .front()
        .ok_or_else(|| format!("unexpected provider turn for model `{model_name}`"))?;

    turn.assert_request(&request_body, messages)?;
    let turn = model
        .turns
        .pop_front()
        .expect("provider turn should still exist after request assertion");

    Ok(turn.response.to_http_response())
}

impl ModelTurn {
    fn assert_request(&self, request_body: &Value, messages: &[Value]) -> Result<(), String> {
        if let Some(expected_prompt) = &self.expected_prompt {
            let actual_prompt = messages
                .iter()
                .rev()
                .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .ok_or_else(|| "provider request missing latest user prompt".to_string())?;

            if !actual_prompt.contains(expected_prompt) {
                return Err(format!("expected prompt containing `{expected_prompt}`, got `{actual_prompt}`"));
            }
        }

        if let Some(expected_tools) = &self.expected_tools {
            assert_tool_names(request_body, expected_tools)?;
        }

        for (tool_name, expected_schema) in &self.expected_tool_schemas {
            assert_tool_schema(request_body, tool_name, expected_schema)?;
        }

        for message_assertion in &self.message_assertions {
            message_assertion(messages);
        }

        Ok(())
    }
}

impl ModelTurnResponse {
    fn to_http_response(&self) -> String {
        match self {
            Self::Error(message) => http_json_response(400, json!({ "error": { "message": message } })),
            _ => http_sse_response(self.to_openai_stream_events()),
        }
    }

    fn to_openai_stream_events(&self) -> Vec<Value> {
        match self {
            Self::Json(output) => openai_tool_call_stream_events([ToolCall::new(
                "finalize",
                json!({
                    "type": "success",
                    "output": output,
                }),
            )]),
            Self::Text(output) => openai_content_stream_events(output.clone()),
            Self::ToolCalls(tool_calls) => openai_tool_call_stream_events(tool_calls.clone()),
            Self::Error(message) => vec![json!({ "error": { "message": message } })],
        }
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
    let mut request_line = String::new();
    let mut headers = BTreeMap::new();
    let mut content_length = 0_usize;
    let mut header_line = String::new();

    reader.read_line(&mut request_line).expect("request line should read");

    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts.next()?.to_string();
    let path = request_line_parts.next()?.to_string();

    loop {
        header_line.clear();
        reader.read_line(&mut header_line).expect("header line should read");

        if header_line == "\r\n" || header_line.is_empty() {
            break;
        }

        if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
            let normalized_header_name = header_name.to_ascii_lowercase();
            let normalized_header_value = header_value.trim().to_string();

            if normalized_header_name == "content-length" {
                content_length = normalized_header_value.parse().expect("content length should parse");
            }

            headers.insert(normalized_header_name, normalized_header_value);
        }
    }

    let mut request_body = vec![0_u8; content_length];
    reader.read_exact(&mut request_body).expect("request body should read");

    Some(HttpRequest {
        method,
        path,
        headers,
        body: request_body,
    })
}

fn http_json_response(status: u16, body: Value) -> String {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let body = body.to_string();

    format!(
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn http_sse_response(events: Vec<Value>) -> String {
    let mut body = String::new();

    for event in events {
        body.push_str("data: ");
        body.push_str(&event.to_string());
        body.push('\n');
    }

    body.push_str("data: [DONE]\n");

    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn openai_content_stream_events(content: String) -> Vec<Value> {
    vec![json!({
        "choices": [{
            "index": 0,
            "delta": {
                "content": content,
            },
            "finish_reason": null,
        }]
    })]
}

fn openai_tool_call_stream_events(tool_calls: impl IntoIterator<Item = ToolCall>) -> Vec<Value> {
    vec![json!({
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": tool_calls.into_iter().enumerate().map(|(index, tool_call)| {
                    json!({
                        "index": index,
                        "id": format!("call_{}", index + 1),
                        "type": "function",
                        "function": {
                            "name": tool_call.name,
                            "arguments": serde_json::to_string(&tool_call.arguments).expect("tool call arguments should serialize"),
                        }
                    })
                }).collect::<Vec<_>>()
            },
            "finish_reason": null,
        }]
    })]
}

fn assert_tool_names(request_body: &Value, expected_tools: &[String]) -> Result<(), String> {
    let mut actual_tools = request_body
        .get("tools")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .filter(|tool_name| *tool_name != "finalize")
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut expected_tools = expected_tools.to_vec();

    actual_tools.sort();
    expected_tools.sort();

    if actual_tools != expected_tools {
        return Err(format!("expected tools {expected_tools:?}, got {actual_tools:?}"));
    }

    Ok(())
}

fn assert_tool_schema(request_body: &Value, tool_name: &str, expected_schema: &Value) -> Result<(), String> {
    let actual_schema = request_body
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool.pointer("/function/name").and_then(Value::as_str) == Some(tool_name))
        })
        .and_then(|tool| tool.pointer("/function/parameters"))
        .ok_or_else(|| format!("tool `{tool_name}` not found in provider request"))?;

    if actual_schema != expected_schema {
        return Err(format!(
            "expected schema for tool `{tool_name}` to be {expected_schema}, got {actual_schema}"
        ));
    }

    Ok(())
}

fn verify_provider_servers(provider_servers: &BTreeMap<String, ProviderServer>) -> BTreeMap<String, Vec<Value>> {
    provider_servers
        .iter()
        .map(|(provider_name, server)| {
            let state = server.state.lock().expect("provider state lock should not be poisoned");

            assert!(state.errors.is_empty(), "provider `{provider_name}` errors: {:?}", state.errors);
            assert_eq!(
                state.uploaded_files, state.deleted_files,
                "provider `{provider_name}` should delete every uploaded file"
            );

            for (model_name, model) in &state.models {
                assert!(
                    model.turns.is_empty(),
                    "provider `{provider_name}` model `{model_name}` has {} unused scripted turns",
                    model.turns.len()
                );
            }

            (provider_name.clone(), state.requests.clone())
        })
        .collect()
}

fn collect_mcp_requests(mcp_client_factory: &FakeMcpClientFactory, server_names: &[String]) -> BTreeMap<String, Vec<FakeMcpRequest>> {
    server_names
        .iter()
        .map(|server_name| {
            let unused_tool_response_counts = mcp_client_factory.unused_tool_response_counts(server_name);

            assert!(
                unused_tool_response_counts.is_empty(),
                "MCP `{server_name}` has unused scripted tool responses: {unused_tool_response_counts:?}"
            );

            (server_name.clone(), mcp_client_factory.requests(server_name))
        })
        .collect()
}

fn replace_block_property(source_text: &str, declaration_keyword: &str, block_name: &str, property_name: &str, value: &Value) -> String {
    let mut output_lines = Vec::new();
    let mut inside_block = false;
    let mut block_depth = 0_usize;
    let mut replaced_property = false;
    let property_value = render_wire_value(value);
    let block_start = format!("{declaration_keyword} {block_name}");

    for line in source_text.lines() {
        let trimmed_line = line.trim();

        if !inside_block && trimmed_line.starts_with(&block_start) && trimmed_line.ends_with('{') {
            inside_block = true;
            block_depth = 1;
            replaced_property = false;
            output_lines.push(line.to_string());
            continue;
        }

        if inside_block {
            if block_depth == 1 && trimmed_line.starts_with(&format!("{property_name}:")) {
                let indentation = line.chars().take_while(|character| character.is_whitespace()).collect::<String>();
                output_lines.push(format!("{indentation}{property_name}: {property_value}"));
                replaced_property = true;
                continue;
            }

            block_depth += count_character(line, '{');
            block_depth = block_depth.saturating_sub(count_character(line, '}'));

            if block_depth == 0 {
                if !replaced_property {
                    output_lines.push(format!("    {property_name}: {property_value}"));
                }

                inside_block = false;
            }
        }

        output_lines.push(line.to_string());
    }

    let mut output = output_lines.join("\n");
    output.push('\n');
    output
}

fn count_character(line: &str, expected_character: char) -> usize {
    line.chars().filter(|character| *character == expected_character).count()
}

fn render_wire_value(value: &Value) -> String {
    match value {
        Value::String(string_value) => serde_json::to_string(string_value).expect("string should serialize"),
        _ => value.to_string(),
    }
}
