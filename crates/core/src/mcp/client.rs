use crate::mcp::schema::to_json_value;
use crate::mcp::{McpError, McpServerConfig, McpServerLock, McpToolLock};
use rust_mcp_schema::{
    CallToolRequest, CallToolRequestParams, ClientCapabilities, GetPromptRequest, GetPromptRequestParams, Implementation,
    InitializeRequest, InitializeRequestParams, InitializedNotification, JsonrpcResponse, ListPromptsRequest, ListPromptsResult,
    ListResourcesRequest, ListResourcesResult, ListToolsRequest, ListToolsResult, ReadResourceRequest, ReadResourceRequestParams,
    RequestId, LATEST_PROTOCOL_VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

macro_rules! mcp_request {
    ($request_type:ident, $request_id:expr $(, $params:expr)?) => {
        $request_type::new(RequestId::Integer($request_id) $(, $params)?)
    };
}

#[derive(Debug)]
pub struct McpClient {
    server_config: McpServerConfig,
    initialized: AtomicBool,
    resource_uri_cache: Mutex<BTreeMap<String, String>>,
}

impl McpClient {
    #[must_use]
    pub fn new(server_config: McpServerConfig) -> Self {
        Self {
            server_config,
            initialized: AtomicBool::new(false),
            resource_uri_cache: Mutex::new(BTreeMap::new()),
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

        self.request_value(InitializeRequest::method_value(), &initialize_request)?;
        self.notify(&InitializedNotification::new(None))?;

        Ok(())
    }

    pub fn list_tools(&self) -> Result<McpServerLock, McpError> {
        log::debug!("MCP tools/list: server={}", self.server_config.name);
        self.ensure_initialized()?;
        let list_tools_result =
            self.request_result::<ListToolsResult, _>(ListToolsRequest::method_value(), &mcp_request!(ListToolsRequest, 2, None))?;
        let mut server_lock = McpServerLock::from(list_tools_result);

        server_lock.resources = self.list_resource_names().unwrap_or_default();
        server_lock.prompts = self.list_prompt_names().unwrap_or_default();

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

    fn list_prompt_names(&self) -> Result<Vec<String>, McpError> {
        let list_prompts_result =
            self.request_result::<ListPromptsResult, _>(ListPromptsRequest::method_value(), &mcp_request!(ListPromptsRequest, 2, None))?;
        let mut prompt_names = list_prompts_result
            .prompts
            .into_iter()
            .map(|prompt| prompt.name)
            .collect::<Vec<_>>();

        prompt_names.sort();
        prompt_names.dedup();

        Ok(prompt_names)
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
                arguments: string_arguments(arguments),
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
        let mut request = ureq::post(&self.server_config.endpoint).header("content-type", "application/json");

        for (header_name, header_value) in &self.server_config.headers {
            request = request.header(header_name, header_value);
        }

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
        let mut request = ureq::post(&self.server_config.endpoint).header("content-type", "application/json");

        for (header_name, header_value) in &self.server_config.headers {
            request = request.header(header_name, header_value);
        }

        let mut response = request.send_json(body).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        response.body_mut().read_json::<Value>().map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpClientPool {
    clients: Arc<Mutex<BTreeMap<String, Arc<McpClient>>>>,
}

impl McpClientPool {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            clients: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn from_server_configs(configs: impl IntoIterator<Item = McpServerConfig>) -> Result<Self, McpError> {
        let mut clients = BTreeMap::new();

        for server_config in configs {
            log::debug!("initializing MCP client pool for server: {}", server_config.name);
            let client = McpClient::new(server_config.clone());
            client.ensure_initialized()?;
            clients.insert(server_config.name, Arc::new(client));
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    pub fn from_workflow(workflow: &crate::dsl::Workflow) -> Result<Self, McpError> {
        let mut clients = BTreeMap::new();

        for server_config in McpServerConfig::from_workflow(workflow)? {
            log::debug!("initializing MCP client pool for server: {}", server_config.name);
            let client = McpClient::new(server_config.clone());
            client.ensure_initialized()?;
            clients.insert(server_config.name, Arc::new(client));
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    pub fn from_workflow_with_context(
        workflow: &crate::dsl::Workflow,
        evaluation_context: &crate::semantic::support::expression::EvaluationContext,
    ) -> Result<Self, McpError> {
        let mut clients = BTreeMap::new();

        for declaration in workflow.declarations() {
            let crate::dsl::Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context)?;
            log::debug!("initializing MCP client pool for runtime server: {}", server_config.name);
            let client = McpClient::new(server_config.clone());
            client.ensure_initialized()?;
            clients.insert(server_config.name, Arc::new(client));
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    pub fn get(&self, server_config: &McpServerConfig) -> Result<Arc<McpClient>, McpError> {
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

fn string_arguments(arguments: Value) -> Option<BTreeMap<String, String>> {
    Some(
        arguments
            .as_object()?
            .iter()
            .filter_map(|(argument_name, argument_value)| {
                argument_value
                    .as_str()
                    .map(|argument_value| (argument_name.clone(), argument_value.to_string()))
            })
            .collect(),
    )
}
