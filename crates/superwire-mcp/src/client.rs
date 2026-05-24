use crate::schema::to_json_value;
use crate::{McpError, McpPromptArgumentLock, McpServerConfig, McpServerLock, McpToolLock};
use rust_mcp_schema::{
    CallToolRequest, CallToolRequestParams, ClientCapabilities, GetPromptRequest, GetPromptRequestParams, Implementation,
    InitializeRequest, InitializeRequestParams, InitializedNotification, JsonrpcResponse, ListPromptsRequest, ListPromptsResult,
    ListResourcesRequest, ListResourcesResult, ListToolsRequest, ListToolsResult, ReadResourceRequest, ReadResourceRequestParams,
    RequestId, LATEST_PROTOCOL_VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const MCP_ACCEPT_HEADER: &str = "application/json, text/event-stream";

macro_rules! mcp_request {
    ($request_type:ident, $request_id:expr $(, $params:expr)?) => {
        $request_type::new(RequestId::Integer($request_id) $(, $params)?)
    };
}

#[derive(Debug)]
pub struct McpClient {
    server_config: McpServerConfig,
    initialized: AtomicBool,
    resource_uri_cache: Mutex<HashMap<String, String>>,
}

pub trait McpClientBackend: fmt::Debug + Send + Sync {
    fn list_tools(&self) -> Result<McpServerLock, McpError>;

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError>;

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError>;

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError>;
}

pub trait McpClientFactory: fmt::Debug + Send + Sync {
    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HttpMcpClientFactory;

impl McpClient {
    #[must_use]
    pub fn new(server_config: McpServerConfig) -> Self {
        Self {
            server_config,
            initialized: AtomicBool::new(false),
            resource_uri_cache: Mutex::new(HashMap::new()),
        }
    }

    fn ensure_initialized(&self) -> Result<(), McpError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        self.send_initialize()?;
        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    fn send_initialize(&self) -> Result<(), McpError> {
        let initialize_request = mcp_request!(
            InitializeRequest,
            1,
            InitializeRequestParams {
                capabilities: ClientCapabilities::default(),
                client_info: Implementation {
                    description: None,
                    icons: Vec::new(),
                    name: "superwire".to_string(),
                    title: None,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    website_url: None,
                },
                meta: None,
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
            }
        );

        if let Err(initialize_error) = self.request_value(InitializeRequest::method_value(), &initialize_request) {
            if initialize_error.is_http_status(406) {
                log::warn!(
                    "MCP initialize not accepted by server {}; continuing without initialize handshake: {}",
                    self.server_config.name,
                    initialize_error
                );

                return Ok(());
            }

            return Err(initialize_error);
        }

        if let Err(notification_error) = self.notify(&InitializedNotification::new(None)) {
            if notification_error.is_http_status(406) {
                log::warn!(
                    "MCP initialized notification not accepted by server {}; continuing: {}",
                    self.server_config.name,
                    notification_error
                );

                return Ok(());
            }

            return Err(notification_error);
        }

        Ok(())
    }

    pub fn list_tools(&self) -> Result<McpServerLock, McpError> {
        log::debug!("MCP tools/list: server={}", self.server_config.name);
        self.ensure_initialized()?;
        let list_tools_result =
            self.request_result::<ListToolsResult, _>(ListToolsRequest::method_value(), &mcp_request!(ListToolsRequest, 2, None))?;
        let mut server_lock = McpServerLock::from(list_tools_result);

        server_lock.resources = self.list_resource_names().unwrap_or_default();
        let prompt_arguments = self.list_prompt_arguments().unwrap_or_default();
        server_lock.prompts = prompt_arguments.keys().cloned().collect();
        server_lock.prompt_arguments = prompt_arguments;

        log::info!(
            "MCP tools/list completed: server={}, tools={}",
            self.server_config.name,
            server_lock.tools.len()
        );

        Ok(server_lock)
    }

    fn list_resource_names(&self) -> Result<Vec<String>, McpError> {
        let list_resources_result = self
            .request_result::<ListResourcesResult, _>(ListResourcesRequest::method_value(), &mcp_request!(ListResourcesRequest, 2, None))?;
        let mut resource_names = list_resources_result
            .resources
            .into_iter()
            .map(|resource| resource.name)
            .collect::<Vec<_>>();

        resource_names.sort();
        resource_names.dedup();

        Ok(resource_names)
    }

    fn list_prompt_arguments(&self) -> Result<BTreeMap<String, Vec<McpPromptArgumentLock>>, McpError> {
        let list_prompts_result =
            self.request_result::<ListPromptsResult, _>(ListPromptsRequest::method_value(), &mcp_request!(ListPromptsRequest, 2, None))?;
        let prompt_arguments = list_prompts_result
            .prompts
            .into_iter()
            .map(|prompt| {
                let arguments = prompt
                    .arguments
                    .into_iter()
                    .map(|argument| McpPromptArgumentLock {
                        name: argument.name,
                        required: argument.required.unwrap_or(false),
                        description: argument.description,
                    })
                    .collect::<Vec<_>>();

                (prompt.name, arguments)
            })
            .collect::<BTreeMap<_, _>>();

        Ok(prompt_arguments)
    }

    pub fn list_resources(&self) -> Result<BTreeMap<String, String>, McpError> {
        log::debug!("MCP resources/list: server={}", self.server_config.name);
        self.ensure_initialized()?;
        self.fetch_resources()
    }

    fn fetch_resources(&self) -> Result<BTreeMap<String, String>, McpError> {
        let list_resources_result = self
            .request_result::<ListResourcesResult, _>(ListResourcesRequest::method_value(), &mcp_request!(ListResourcesRequest, 2, None))?;
        let name_to_uri = list_resources_result
            .resources
            .into_iter()
            .map(|resource| (resource.name, resource.uri))
            .collect::<BTreeMap<_, _>>();

        log::info!(
            "MCP resources/list completed: server={}, resources={}",
            self.server_config.name,
            name_to_uri.len()
        );

        Ok(name_to_uri)
    }

    pub fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        log::debug!("MCP tools/call: server={}, tool={tool_name}", self.server_config.name);
        self.ensure_initialized()?;

        let call_tool_request = mcp_request!(
            CallToolRequest,
            2,
            CallToolRequestParams {
                arguments: arguments.as_object().cloned(),
                meta: None,
                name: tool_name.to_string(),
                task: None,
            }
        );
        let result = self.request_value(CallToolRequest::method_value(), &call_tool_request)?;

        log::info!("MCP tools/call completed: server={}, tool={tool_name}", self.server_config.name);

        Ok(result)
    }

    pub fn read_resource(&self, resource_name: &str, _arguments: Value) -> Result<Value, McpError> {
        let resource_uri = self.resolve_resource_uri(resource_name)?;

        log::debug!(
            "MCP resources/read: server={}, resource={resource_name} -> uri={resource_uri}",
            self.server_config.name
        );
        self.ensure_initialized()?;

        let read_resource_request = mcp_request!(
            ReadResourceRequest,
            2,
            ReadResourceRequestParams {
                meta: None,
                uri: resource_uri,
            }
        );
        let result = self.request_value(ReadResourceRequest::method_value(), &read_resource_request)?;

        log::info!(
            "MCP resources/read completed: server={}, resource={resource_name}",
            self.server_config.name
        );

        Ok(result)
    }

    fn resolve_resource_uri(&self, resource_name: &str) -> Result<String, McpError> {
        {
            let cache = self.resource_uri_cache.lock().expect("resource uri cache lock poisoned");

            if let Some(uri) = cache.get(resource_name) {
                return Ok(uri.clone());
            }
        }

        self.ensure_initialized()?;
        let resources = self.fetch_resources()?;
        let uri = resources.get(resource_name).cloned().ok_or_else(|| McpError::Rpc {
            server_name: self.server_config.name.clone(),
            method: ListResourcesRequest::method_value().to_string(),
            message: format!("resource `{resource_name}` not found in server's resource list"),
        })?;

        let mut cache = self.resource_uri_cache.lock().expect("resource uri cache lock poisoned");
        cache.extend(resources);

        Ok(uri)
    }

    pub fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError> {
        log::debug!("MCP prompts/get: server={}, prompt={prompt_name}", self.server_config.name);
        self.ensure_initialized()?;

        let get_prompt_request = mcp_request!(
            GetPromptRequest,
            2,
            GetPromptRequestParams {
                arguments: arguments.to_prompt_string_arguments(),
                meta: None,
                name: prompt_name.to_string(),
            }
        );
        let result = self.request_value(GetPromptRequest::method_value(), &get_prompt_request)?;

        log::info!(
            "MCP prompts/get completed: server={}, prompt={prompt_name}",
            self.server_config.name
        );

        Ok(result)
    }

    fn notify(&self, notification: &impl Serialize) -> Result<(), McpError> {
        let method = InitializedNotification::method_value();
        let request = self.http_post_request();

        request.send_json(notification).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        Ok(())
    }

    fn request_result<Response, Request>(&self, method: &str, request: &Request) -> Result<Response, McpError>
    where
        Response: DeserializeOwned,
        Request: Serialize,
    {
        let result = self.request_value(method, request)?;

        serde_json::from_value(result).map_err(|error| McpError::InvalidResponse {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })
    }

    fn request_value(&self, method: &str, request: &impl Serialize) -> Result<Value, McpError> {
        let response = self.post(method, request)?;

        match serde_json::from_value::<JsonrpcResponse>(response.clone()) {
            Ok(JsonrpcResponse::ErrorResponse(error_response)) => Err(McpError::Rpc {
                server_name: self.server_config.name.clone(),
                method: method.to_string(),
                message: error_response.error.message,
            }),
            Ok(JsonrpcResponse::ResultResponse(result_response)) => Ok(to_json_value(&result_response.result)),
            Err(_error) => {
                if let Some(error) = response.get("error") {
                    return Err(McpError::Rpc {
                        server_name: self.server_config.name.clone(),
                        method: method.to_string(),
                        message: error.to_string(),
                    });
                }

                response.get("result").cloned().ok_or_else(|| McpError::MissingResult {
                    server_name: self.server_config.name.clone(),
                    method: method.to_string(),
                })
            }
        }
    }

    fn post(&self, method: &str, body: &impl Serialize) -> Result<Value, McpError> {
        let request = self.http_post_request();

        let mut response = request.send_json(body).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        let response_body = response.body_mut().read_to_string().map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        Self::parse_response_body(&response_body).map_err(|message| McpError::InvalidResponse {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message,
        })
    }

    fn http_post_request(&self) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
        let mut request = ureq::post(&self.server_config.endpoint);

        for (header_name, header_value) in &self.server_config.headers {
            request = request.header(header_name, header_value);
        }

        request
            .header("content-type", "application/json")
            .header("accept", MCP_ACCEPT_HEADER)
    }

    fn parse_response_body(response_body: &str) -> Result<Value, String> {
        serde_json::from_str(response_body)
            .or_else(|json_error| Self::parse_event_stream_response(response_body).ok_or_else(|| json_error.to_string()))
    }

    fn parse_event_stream_response(response_body: &str) -> Option<Value> {
        let mut event_data = Vec::new();

        for response_line in response_body.lines() {
            let response_line = response_line.trim_end_matches('\r');

            if response_line.is_empty() {
                if let Some(value) = Self::parse_event_data(&event_data) {
                    return Some(value);
                }

                event_data.clear();
                continue;
            }

            if let Some(data) = response_line.strip_prefix("data:") {
                event_data.push(data.trim_start());
            }
        }

        Self::parse_event_data(&event_data)
    }

    fn parse_event_data(event_data: &[&str]) -> Option<Value> {
        if event_data.is_empty() {
            return None;
        }

        let event_body = event_data.join("\n");

        serde_json::from_str(&event_body).ok()
    }
}

impl McpClientBackend for McpClient {
    fn list_tools(&self) -> Result<McpServerLock, McpError> {
        McpClient::list_tools(self)
    }

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        McpClient::call_tool(self, tool_name, arguments)
    }

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError> {
        McpClient::read_resource(self, resource_name, arguments)
    }

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError> {
        McpClient::get_prompt(self, prompt_name, arguments)
    }
}

impl McpClientFactory for HttpMcpClientFactory {
    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let client = McpClient::new(server_config);
        client.ensure_initialized()?;

        Ok(Arc::new(client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[derive(Clone, Copy)]
    enum TestResponseKind {
        Json,
        EventStream,
    }

    #[derive(Debug)]
    struct TestHttpRequest {
        headers: BTreeMap<String, String>,
        body: Value,
    }

    #[test]
    fn sends_json_and_event_stream_accept_header_for_all_mcp_requests() {
        let (endpoint, received_requests, server_thread) = spawn_mcp_server(TestResponseKind::Json);
        let client = McpClient::new(McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        });

        client.list_tools().expect("tools should list");
        server_thread.join().expect("server thread should finish");

        let received_requests = received_requests.lock().expect("received requests lock should not poison");
        let received_methods = received_requests
            .iter()
            .map(|request| request.body.get("method").and_then(Value::as_str).unwrap_or_default())
            .collect::<Vec<_>>();

        assert_eq!(
            received_methods,
            vec![
                InitializeRequest::method_value(),
                InitializedNotification::method_value(),
                ListToolsRequest::method_value(),
                ListResourcesRequest::method_value(),
                ListPromptsRequest::method_value(),
            ]
        );

        for request in received_requests.iter() {
            assert_eq!(request.headers.get("accept").map(String::as_str), Some(MCP_ACCEPT_HEADER));
            assert_eq!(request.headers.get("content-type").map(String::as_str), Some("application/json"));
        }
    }

    #[test]
    fn reads_mcp_json_rpc_results_from_event_stream_responses() {
        let (endpoint, _received_requests, server_thread) = spawn_mcp_server(TestResponseKind::EventStream);
        let client = McpClient::new(McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        });

        let server_lock = client.list_tools().expect("tools should list from event stream responses");

        server_thread.join().expect("server thread should finish");
        assert!(server_lock.tools.contains_key("echo"));
        assert_eq!(server_lock.resources, vec!["project-readme".to_string()]);
        assert_eq!(server_lock.prompts, vec!["summarize".to_string()]);
    }

    fn spawn_mcp_server(response_kind: TestResponseKind) -> (String, Arc<Mutex<Vec<TestHttpRequest>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let received_requests = Arc::new(Mutex::new(Vec::new()));
        let thread_received_requests = Arc::clone(&received_requests);
        let server_thread = thread::spawn(move || {
            for incoming_stream in listener.incoming().take(5) {
                let mut stream = incoming_stream.expect("incoming stream should open");
                let request = read_test_http_request(&stream).expect("HTTP request should read");
                let response = test_response_for_request(&request.body, response_kind);

                thread_received_requests
                    .lock()
                    .expect("received requests lock should not poison")
                    .push(request);
                stream.write_all(response.as_bytes()).expect("HTTP response should write");
            }
        });

        (endpoint, received_requests, server_thread)
    }

    fn read_test_http_request(stream: &TcpStream) -> Option<TestHttpRequest> {
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

        Some(TestHttpRequest { headers, body })
    }

    fn test_response_for_request(request_body: &Value, response_kind: TestResponseKind) -> String {
        let method = request_body.get("method").and_then(Value::as_str).unwrap_or_default();

        if method == InitializedNotification::method_value() {
            return "HTTP/1.1 202 Accepted\r\nconnection: close\r\ncontent-length: 0\r\n\r\n".to_string();
        }

        let response_body = json!({
            "jsonrpc": "2.0",
            "id": request_body.get("id").cloned().unwrap_or_else(|| json!(1)),
            "result": result_for_method(method),
        });

        match response_kind {
            TestResponseKind::Json => http_json_response(response_body),
            TestResponseKind::EventStream => http_event_stream_response(response_body),
        }
    }

    fn result_for_method(method: &str) -> Value {
        match method {
            method if method == InitializeRequest::method_value() => json!({}),
            method if method == ListToolsRequest::method_value() => json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo input",
                    "inputSchema": { "type": "object" },
                }]
            }),
            method if method == ListResourcesRequest::method_value() => json!({
                "resources": [{
                    "name": "project-readme",
                    "uri": "file:///README.md",
                    "mimeType": "text/markdown",
                }]
            }),
            method if method == ListPromptsRequest::method_value() => json!({
                "prompts": [{
                    "name": "summarize",
                    "description": "Summarize text",
                    "arguments": [],
                }]
            }),
            _ => json!({}),
        }
    }

    fn http_json_response(body: Value) -> String {
        let body = body.to_string();

        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn http_event_stream_response(body: Value) -> String {
        let body = format!("event: message\ndata: {body}\n\n");

        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }
}

#[derive(Debug, Clone)]
pub struct McpClientPool {
    clients: Arc<Mutex<HashMap<String, Arc<dyn McpClientBackend>>>>,
}

impl McpClientPool {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_server_configs(configs: impl IntoIterator<Item = McpServerConfig>) -> Result<Self, McpError> {
        Self::from_server_configs_with_factory(configs, &HttpMcpClientFactory)
    }

    pub fn from_server_configs_with_factory(
        configs: impl IntoIterator<Item = McpServerConfig>,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let mut clients = HashMap::new();

        for server_config in configs {
            log::debug!("initializing MCP client pool for server: {}", server_config.name);
            let client = client_factory.client_for_config(server_config.clone())?;
            clients.insert(server_config.name, client);
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    pub fn from_workflow(workflow: &superwire_types::ast::Workflow) -> Result<Self, McpError> {
        Self::from_server_configs(McpServerConfig::from_workflow(workflow)?)
    }

    pub fn from_workflow_with_context(
        workflow: &superwire_types::ast::Workflow,
        evaluation_context: &superwire_semantic::support::expression::EvaluationContext,
    ) -> Result<Self, McpError> {
        Self::from_workflow_with_context_and_factory(workflow, evaluation_context, &HttpMcpClientFactory)
    }

    pub fn from_workflow_with_context_and_factory(
        workflow: &superwire_types::ast::Workflow,
        evaluation_context: &superwire_semantic::support::expression::EvaluationContext,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let mut clients = HashMap::new();

        for declaration in workflow.declarations() {
            let superwire_types::ast::Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context)?;
            log::debug!("initializing MCP client pool for runtime server: {}", server_config.name);
            let client = client_factory.client_for_config(server_config.clone())?;
            clients.insert(server_config.name, client);
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    pub fn from_clients(clients: impl IntoIterator<Item = (String, Arc<dyn McpClientBackend>)>) -> Self {
        Self {
            clients: Arc::new(Mutex::new(clients.into_iter().collect())),
        }
    }

    pub fn get(&self, server_config: &McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let clients = self.clients.lock().expect("mcp client pool lock poisoned");
        let client = clients.get(&server_config.name).ok_or_else(|| McpError::Http {
            server_name: server_config.name.clone(),
            method: "pool".to_string(),
            message: format!("MCP server `{}` not found in client pool", server_config.name),
        })?;

        Ok(Arc::clone(client))
    }
}

impl From<ListToolsResult> for McpServerLock {
    fn from(list_tools_result: ListToolsResult) -> Self {
        let mut server_lock = Self::default();

        for tool in list_tools_result.tools {
            server_lock.tools.insert(
                tool.name.clone(),
                McpToolLock {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool.input_schema,
                    output_schema: tool.output_schema,
                },
            );
        }

        server_lock
    }
}

trait McpPromptArgumentsExt {
    fn to_prompt_string_arguments(&self) -> Option<BTreeMap<String, String>>;

    fn to_prompt_string_argument(&self) -> String;
}

impl McpPromptArgumentsExt for Value {
    fn to_prompt_string_arguments(&self) -> Option<BTreeMap<String, String>> {
        Some(
            self.as_object()?
                .iter()
                .map(|(argument_name, argument_value)| (argument_name.clone(), argument_value.to_prompt_string_argument()))
                .collect(),
        )
    }

    fn to_prompt_string_argument(&self) -> String {
        match self {
            Value::String(string_value) => string_value.clone(),
            Value::Null | Value::Bool(_) | Value::Number(_) => self.to_string(),
            Value::Array(_) | Value::Object(_) => serde_json::to_string(self).expect("JSON value should serialize"),
        }
    }
}
