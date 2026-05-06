use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_core::dsl::format_workflow_source;
use superwire_executor::api::{ExecutionOptions, ExecutionRequest};
use superwire_executor::model::OpenAiModelProvider;
use superwire_executor::runtime::ExecutorError;
use superwire_executor::service::ExecutorService;

type MessageAssertion = Arc<dyn Fn(&[Value]) + Send + Sync>;

#[derive(Debug)]
pub struct TestRunOutput {
    pub output: Value,
    pub provider_requests: BTreeMap<String, Vec<Value>>,
    pub mcp_requests: BTreeMap<String, Vec<Value>>,
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
    expected_response_format: Option<Format>,
    message_assertions: Vec<MessageAssertion>,
    response: Option<ModelTurnResponse>,
}

enum WorkflowSource {
    Inline(String),
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    Auto,
    JsonSchema,
    JsonObject,
    InstructionOnly,
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
    expected_response_format: Option<Format>,
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
}

struct ProviderServer {
    endpoint: String,
    state: Arc<Mutex<ProviderServerState>>,
}

struct ProviderServerState {
    models: BTreeMap<String, ModelScript>,
    requests: Vec<Value>,
    errors: Vec<String>,
}

#[derive(Debug)]
struct McpServer {
    endpoint: String,
    state: Arc<Mutex<McpServerState>>,
}

#[derive(Debug)]
struct McpServerState {
    script: McpScript,
    requests: Vec<Value>,
    errors: Vec<String>,
}

impl TestRunner {
    #[must_use]
    pub fn workflow(workflow_source: impl Into<String>) -> Self {
        Self {
            workflow_source: resolve_workflow_source(workflow_source.into()),
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
        let mcp_servers = self.spawn_mcp_servers();
        let workflow_source = self.workflow_source_with_mock_endpoints(&provider_servers, &mcp_servers)?;
        let service = ExecutorService::new(OpenAiModelProvider);
        let execution_response = service
            .execute(ExecutionRequest {
                workflow_source: Some(workflow_source),
                workflow_source_base64: None,
                input: self.input,
                secrets: self.secrets,
                options: ExecutionOptions {
                    include_events: false,
                    max_concurrency: self.max_concurrency,
                },
            })
            .await?;

        let provider_requests = verify_provider_servers(&provider_servers);
        let mcp_requests = verify_mcp_servers(&mcp_servers);

        Ok(TestRunOutput {
            output: execution_response.output,
            provider_requests,
            mcp_requests,
        })
    }

    fn spawn_provider_servers(&self) -> BTreeMap<String, ProviderServer> {
        self.providers
            .iter()
            .map(|(provider_name, provider_script)| (provider_name.clone(), ProviderServer::spawn(provider_script)))
            .collect()
    }

    fn spawn_mcp_servers(&self) -> BTreeMap<String, McpServer> {
        self.mcp_servers
            .iter()
            .map(|(server_name, script)| (server_name.clone(), McpServer::spawn(script.clone())))
            .collect()
    }

    fn workflow_source_with_mock_endpoints(
        &self,
        provider_servers: &BTreeMap<String, ProviderServer>,
        mcp_servers: &BTreeMap<String, McpServer>,
    ) -> Result<String, ExecutorError> {
        let raw_workflow_source = self.workflow_source.read()?;
        let mut workflow_source = format_workflow_source(&raw_workflow_source).unwrap_or(raw_workflow_source);

        for (provider_name, server) in provider_servers {
            workflow_source = replace_block_property(&workflow_source, "provider", provider_name, "endpoint", &json!(server.endpoint));
        }

        for (server_name, server) in mcp_servers {
            workflow_source = replace_block_property(&workflow_source, "mcp", server_name, "endpoint", &json!(server.endpoint));
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

    pub fn with_response_format(mut self, response_format: Format) -> Self {
        self.turn.expected_response_format = Some(response_format);
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
            expected_response_format: self.expected_response_format.clone(),
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
            input_schema: self.input_schema.unwrap_or_else(empty_object_schema),
            output_schema: self.output_schema.unwrap_or_else(empty_object_schema),
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

    fn build(self) -> McpPromptScript {
        McpPromptScript {
            description: self.description.unwrap_or_else(|| "Test MCP prompt".to_string()),
            text: self.text.unwrap_or_default(),
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
        }
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
            expected_response_format: self.expected_response_format.clone(),
            message_assertions: self.message_assertions.clone(),
            response: self.response.clone(),
        }
    }
}

impl McpServer {
    fn spawn(script: McpScript) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("MCP test listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("MCP local address should exist"));
        let state = Arc::new(Mutex::new(McpServerState {
            script,
            requests: Vec::new(),
            errors: Vec::new(),
        }));
        let thread_state = Arc::clone(&state);

        thread::spawn(move || {
            for incoming_stream in listener.incoming() {
                let stream = incoming_stream.expect("MCP test stream should open");
                handle_mcp_request(stream, &thread_state);
            }
        });

        Self { endpoint, state }
    }
}

fn handle_provider_request(mut stream: TcpStream, state: &Arc<Mutex<ProviderServerState>>, expected_api_key: &str) {
    let Some(request) = read_http_json_request(&stream) else {
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
    request: &HttpJsonRequest,
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

    let model_name = request
        .body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| "provider request missing model".to_string())?;
    let messages = request
        .body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "provider request missing messages".to_string())?;
    let mut state = state.lock().expect("provider state lock should not be poisoned");
    state.requests.push(request.body.clone());
    let model = state
        .models
        .get_mut(model_name)
        .ok_or_else(|| format!("unexpected provider model `{model_name}`"))?;
    let turn = model
        .turns
        .pop_front()
        .ok_or_else(|| format!("unexpected provider turn for model `{model_name}`"))?;

    turn.assert_request(&request.body, messages)?;

    Ok(http_json_response(200, turn.response.to_openai_response()))
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

        if let Some(expected_response_format) = self.expected_response_format.clone() {
            expected_response_format.assert_request(request_body)?;
        }

        for message_assertion in &self.message_assertions {
            message_assertion(messages);
        }

        Ok(())
    }
}

impl Format {
    fn assert_request(self, request_body: &Value) -> Result<(), String> {
        match self {
            Self::Auto => {
                let actual_response_format_type = request_body.pointer("/response_format/type").and_then(Value::as_str);

                if actual_response_format_type != Some("json_schema") {
                    return Err(format!(
                        "expected auto response format first request to use `json_schema`, got {actual_response_format_type:?}"
                    ));
                }
            }
            Self::JsonSchema => {
                let actual_response_format_type = request_body.pointer("/response_format/type").and_then(Value::as_str);

                if actual_response_format_type != Some("json_schema") {
                    return Err(format!(
                        "expected response_format type `json_schema`, got {actual_response_format_type:?}"
                    ));
                }
            }
            Self::JsonObject => {
                let actual_response_format_type = request_body.pointer("/response_format/type").and_then(Value::as_str);

                if actual_response_format_type != Some("json_object") {
                    return Err(format!(
                        "expected response_format type `json_object`, got {actual_response_format_type:?}"
                    ));
                }
            }
            Self::InstructionOnly => {
                if request_body.get("response_format").is_some() {
                    return Err(format!(
                        "expected response_format to be omitted, got {:?}",
                        request_body.get("response_format")
                    ));
                }
            }
        }

        Ok(())
    }
}

impl ModelTurnResponse {
    fn to_openai_response(&self) -> Value {
        match self {
            Self::Json(output) => openai_content_response(serde_json::to_string(output).expect("scripted output should serialize")),
            Self::Text(output) => openai_content_response(output.clone()),
            Self::ToolCalls(tool_calls) => json!({
                "id": "chatcmpl_test",
                "object": "chat.completion",
                "created": 1,
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "tool_calls": tool_calls.iter().enumerate().map(|(index, tool_call)| {
                            json!({
                                "id": format!("call_{}", index + 1),
                                "type": "function",
                                "function": {
                                    "name": tool_call.name,
                                    "arguments": serde_json::to_string(&tool_call.arguments).expect("tool call arguments should serialize"),
                                }
                            })
                        }).collect::<Vec<_>>()
                    }
                }]
            }),
        }
    }
}

fn handle_mcp_request(mut stream: TcpStream, state: &Arc<Mutex<McpServerState>>) {
    let Some(request) = read_http_json_request(&stream) else {
        return;
    };
    let response = build_mcp_response(&request.body, state).unwrap_or_else(|message| {
        state
            .lock()
            .expect("MCP state lock should not be poisoned")
            .errors
            .push(message.clone());
        http_json_response(
            500,
            json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32000, "message": message } }),
        )
    });

    stream.write_all(response.as_bytes()).expect("MCP response should write");
}

fn build_mcp_response(request_body: &Value, state: &Arc<Mutex<McpServerState>>) -> Result<String, String> {
    let method = request_body.get("method").and_then(Value::as_str).unwrap_or_default();
    let request_id = request_body.get("id").cloned().unwrap_or_else(|| json!(1));
    let mut state = state.lock().expect("MCP state lock should not be poisoned");
    state.requests.push(request_body.clone());

    let Some(result) = state.result_for_method(method, request_body)? else {
        return Ok("HTTP/1.1 202 Accepted\r\nconnection: close\r\ncontent-length: 0\r\n\r\n".to_string());
    };

    Ok(http_json_response(
        200,
        json!({ "jsonrpc": "2.0", "id": request_id, "result": result }),
    ))
}

impl McpServerState {
    fn result_for_method(&mut self, method: &str, request_body: &Value) -> Result<Option<Value>, String> {
        match method {
            "notifications/initialized" => Ok(None),
            "tools/list" => Ok(Some(json!({ "tools": self.script.tool_list() }))),
            "resources/list" => Ok(Some(json!({ "resources": self.script.resource_list() }))),
            "prompts/list" => Ok(Some(json!({ "prompts": self.script.prompt_list() }))),
            "resources/read" => self.resource_read_result(request_body).map(Some),
            "prompts/get" => self.prompt_get_result(request_body).map(Some),
            "tools/call" => self.tool_call_result(request_body).map(Some),
            _ => Ok(Some(json!({}))),
        }
    }

    fn tool_call_result(&mut self, request_body: &Value) -> Result<Value, String> {
        let tool_name = request_body
            .pointer("/params/name")
            .and_then(Value::as_str)
            .ok_or_else(|| "MCP tools/call missing params.name".to_string())?;
        let tool = self
            .script
            .tools
            .get_mut(tool_name)
            .ok_or_else(|| format!("unexpected MCP tool call `{tool_name}`"))?;
        let response = tool
            .responses
            .pop_front()
            .ok_or_else(|| format!("unexpected extra call to MCP tool `{tool_name}`"))?;

        Ok(json!({
            "content": [{ "type": "text", "text": serde_json::to_string(&response).expect("tool response should serialize") }],
            "structuredContent": response,
        }))
    }

    fn resource_read_result(&self, request_body: &Value) -> Result<Value, String> {
        let uri = request_body
            .pointer("/params/uri")
            .and_then(Value::as_str)
            .ok_or_else(|| "MCP resources/read missing params.uri".to_string())?;
        let resource = self
            .script
            .resources
            .values()
            .find(|resource| resource.uri == uri)
            .ok_or_else(|| format!("unexpected MCP resource uri `{uri}`"))?;

        Ok(json!({
            "contents": [{
                "uri": resource.uri,
                "mimeType": resource.mime_type,
                "text": resource.text,
            }]
        }))
    }

    fn prompt_get_result(&self, request_body: &Value) -> Result<Value, String> {
        let prompt_name = request_body
            .pointer("/params/name")
            .and_then(Value::as_str)
            .ok_or_else(|| "MCP prompts/get missing params.name".to_string())?;
        let prompt = self
            .script
            .prompts
            .get(prompt_name)
            .ok_or_else(|| format!("unexpected MCP prompt `{prompt_name}`"))?;

        Ok(json!({
            "description": prompt.description,
            "messages": [{
                "role": "user",
                "content": { "type": "text", "text": prompt.text }
            }]
        }))
    }
}

impl McpScript {
    fn tool_list(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|(tool_name, tool)| {
                json!({
                    "name": tool_name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                    "outputSchema": tool.output_schema,
                })
            })
            .collect()
    }

    fn resource_list(&self) -> Vec<Value> {
        self.resources
            .iter()
            .map(|(resource_name, resource)| {
                json!({
                    "name": resource_name,
                    "uri": resource.uri,
                    "mimeType": resource.mime_type,
                })
            })
            .collect()
    }

    fn prompt_list(&self) -> Vec<Value> {
        self.prompts
            .iter()
            .map(|(prompt_name, prompt)| {
                json!({
                    "name": prompt_name,
                    "description": prompt.description,
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct HttpJsonRequest {
    headers: BTreeMap<String, String>,
    body: Value,
}

fn read_http_json_request(stream: &TcpStream) -> Option<HttpJsonRequest> {
    let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
    let mut headers = BTreeMap::new();
    let mut content_length = 0_usize;
    let mut header_line = String::new();

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

    if content_length == 0 {
        return None;
    }

    let mut request_body = vec![0_u8; content_length];
    reader.read_exact(&mut request_body).expect("request body should read");
    let body = serde_json::from_slice(&request_body).expect("request body should be JSON");

    Some(HttpJsonRequest { headers, body })
}

fn http_json_response(status: u16, body: Value) -> String {
    let status_text = match status {
        200 => "OK",
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

fn openai_content_response(content: String) -> Value {
    json!({
        "id": "chatcmpl_test",
        "object": "chat.completion",
        "created": 1,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {
                "role": "assistant",
                "content": content,
            }
        }]
    })
}

fn assert_tool_names(request_body: &Value, expected_tools: &[String]) -> Result<(), String> {
    let mut actual_tools = request_body
        .get("tools")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
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

fn verify_mcp_servers(mcp_servers: &BTreeMap<String, McpServer>) -> BTreeMap<String, Vec<Value>> {
    mcp_servers
        .iter()
        .map(|(server_name, server)| {
            let state = server.state.lock().expect("MCP state lock should not be poisoned");

            assert!(state.errors.is_empty(), "MCP `{server_name}` errors: {:?}", state.errors);

            for (tool_name, tool) in &state.script.tools {
                assert!(
                    tool.responses.is_empty(),
                    "MCP `{server_name}` tool `{tool_name}` has {} unused scripted responses",
                    tool.responses.len()
                );
            }

            (server_name.clone(), state.requests.clone())
        })
        .collect()
}

impl WorkflowSource {
    fn read(&self) -> Result<String, ExecutorError> {
        match self {
            Self::Inline(workflow_source) => Ok(workflow_source.clone()),
            Self::File(workflow_path) => fs::read_to_string(workflow_path).map_err(|error| ExecutorError::Other {
                message: format!("failed to read workflow fixture {}: {error}", workflow_path.display()),
            }),
        }
    }
}

fn resolve_workflow_source(workflow_source: String) -> WorkflowSource {
    if workflow_source.contains('\n')
        || workflow_source.trim_start().starts_with("provider")
        || workflow_source.trim_start().starts_with("mcp")
    {
        return WorkflowSource::Inline(workflow_source);
    }

    let workflow_path = PathBuf::from(&workflow_source);

    if workflow_path.exists() {
        return WorkflowSource::File(workflow_path);
    }

    WorkflowSource::File(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(workflow_source),
    )
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

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "required": [],
        "additionalProperties": false,
    })
}
