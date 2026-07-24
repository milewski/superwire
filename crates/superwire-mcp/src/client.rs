use crate::blocking::{authorize_current_mcp_http_dispatch, McpBlockingExecutor, McpBlockingOperation};
use crate::network::{McpEndpointApproval, SystemMcpDnsResolver, MCP_HTTP_MAX_RESPONSE_BODY_BYTES};
use crate::schema::to_json_value;
use crate::{McpDnsResolver, McpError, McpNetworkPolicy, McpPromptArgumentLock, McpServerConfig, McpServerLock, McpToolLock};
use rust_mcp_schema::{
    CallToolRequest, CallToolRequestParams, ClientCapabilities, GetPromptRequest, GetPromptRequestParams, Implementation,
    InitializeRequest, InitializeRequestParams, InitializedNotification, JsonrpcResponse, ListPromptsRequest, ListPromptsResult,
    ListResourceTemplatesRequest, ListResourceTemplatesResult, ListResourcesRequest, ListResourcesResult, ListToolsRequest,
    ListToolsResult, ReadResourceRequest, ReadResourceRequestParams, RequestId, ResourceTemplate, LATEST_PROTOCOL_VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use superwire_semantic::support::expression::EvaluationContext;
use superwire_types::ast::{Declaration, Workflow};
const MCP_ACCEPT_HEADER: &str = "application/json, text/event-stream";

macro_rules! mcp_request {
    ($request_type:ident, $request_id:expr $(, $params:expr)?) => {
        $request_type::new(RequestId::Integer($request_id) $(, $params)?)
    };
}

#[derive(Debug, Clone)]
enum McpResourceLocator {
    Static(String),
    Template(McpResourceTemplate),
}

impl McpResourceLocator {
    fn resolve_uri(&self, arguments: &Value, server_name: &str, resource_name: &str) -> Result<String, McpError> {
        match self {
            Self::Static(uri) => {
                if arguments.is_null() || arguments.as_object().is_some_and(serde_json::Map::is_empty) {
                    return Ok(uri.clone());
                }

                Err(McpError::InvalidResourceArguments {
                    server_name: server_name.to_string(),
                    resource_name: resource_name.to_string(),
                    message: "the server advertises a static resource URI, so bindings are not supported".to_string(),
                })
            }
            Self::Template(resource_template) => resource_template.expand(arguments, server_name, resource_name),
        }
    }
}

#[derive(Debug, Clone)]
struct McpResourceTemplate {
    uri_template: String,
    variable_names: BTreeSet<String>,
    required_variable_names: BTreeSet<String>,
}

impl McpResourceTemplate {
    fn from_schema(resource_template: ResourceTemplate) -> Result<Self, String> {
        let uri_template = resource_template.uri_template;
        let mut variable_names = BTreeSet::new();
        let mut required_variable_names = BTreeSet::new();
        let mut remaining_template = uri_template.as_str();

        while let Some(expression_start) = remaining_template.find('{') {
            let expression_tail = &remaining_template[expression_start + 1..];
            let expression_end = expression_tail
                .find('}')
                .ok_or_else(|| "resource URI template contains an unterminated expression".to_string())?;
            let expression = &expression_tail[..expression_end];
            let expression_operator = expression
                .chars()
                .next()
                .filter(|character| matches!(character, '+' | '#' | '.' | '/' | ';' | '?' | '&'));
            let variable_list = expression
                .strip_prefix(|character| matches!(character, '+' | '#' | '.' | '/' | ';' | '?' | '&'))
                .unwrap_or(expression);

            for variable_specification in variable_list.split(',') {
                let variable_name = variable_specification
                    .trim_end_matches('*')
                    .split_once(':')
                    .map_or(variable_specification.trim_end_matches('*'), |(name, _prefix_length)| name);

                if variable_name.is_empty() {
                    return Err("resource URI template contains an empty variable name".to_string());
                }

                variable_names.insert(variable_name.to_string());

                if !matches!(expression_operator, Some('?' | '&')) {
                    required_variable_names.insert(variable_name.to_string());
                }
            }

            remaining_template = &expression_tail[expression_end + 1..];
        }

        if remaining_template.contains('}') {
            return Err("resource URI template contains an unmatched closing brace".to_string());
        }

        stduritemplate::expand(&uri_template, &HashMap::new()).map_err(|error| error.to_string())?;

        Ok(Self {
            uri_template,
            variable_names,
            required_variable_names,
        })
    }

    fn expand(&self, arguments: &Value, server_name: &str, resource_name: &str) -> Result<String, McpError> {
        let argument_object = arguments.as_object().ok_or_else(|| McpError::InvalidResourceArguments {
            server_name: server_name.to_string(),
            resource_name: resource_name.to_string(),
            message: "resource template arguments must be an object".to_string(),
        })?;
        let argument_names = argument_object.keys().cloned().collect::<BTreeSet<_>>();
        let missing_arguments = self
            .required_variable_names
            .difference(&argument_names)
            .cloned()
            .collect::<Vec<_>>();
        let unexpected_arguments = argument_names.difference(&self.variable_names).cloned().collect::<Vec<_>>();

        if !missing_arguments.is_empty() || !unexpected_arguments.is_empty() {
            let mut problems = Vec::new();

            if !missing_arguments.is_empty() {
                problems.push(format!("missing required arguments: {}", missing_arguments.join(", ")));
            }

            if !unexpected_arguments.is_empty() {
                problems.push(format!("unexpected arguments: {}", unexpected_arguments.join(", ")));
            }

            return Err(McpError::InvalidResourceArguments {
                server_name: server_name.to_string(),
                resource_name: resource_name.to_string(),
                message: problems.join("; "),
            });
        }

        let substitutions = argument_object
            .iter()
            .map(|(argument_name, argument_value)| {
                Self::template_value(argument_value)
                    .map(|template_value| (argument_name.clone(), template_value))
                    .map_err(|message| McpError::InvalidResourceArguments {
                        server_name: server_name.to_string(),
                        resource_name: resource_name.to_string(),
                        message: format!("argument `{argument_name}` {message}"),
                    })
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        stduritemplate::expand(&self.uri_template, &substitutions).map_err(|error| McpError::InvalidResourceArguments {
            server_name: server_name.to_string(),
            resource_name: resource_name.to_string(),
            message: error.to_string(),
        })
    }

    fn template_value(value: &Value) -> Result<stduritemplate::Value, String> {
        match value {
            Value::Null => Err("must not be null".to_string()),
            Value::Bool(boolean_value) => Ok(stduritemplate::Value::Bool(*boolean_value)),
            Value::Number(number_value) => {
                if let Some(integer_value) = number_value.as_i64() {
                    Ok(stduritemplate::Value::Integer(integer_value))
                } else if let Some(float_value) = number_value.as_f64() {
                    Ok(stduritemplate::Value::Float(float_value))
                } else {
                    Ok(stduritemplate::Value::String(number_value.to_string()))
                }
            }
            Value::String(string_value) => Ok(stduritemplate::Value::String(string_value.clone())),
            Value::Array(array_values) => array_values
                .iter()
                .map(Self::template_value)
                .collect::<Result<Vec<_>, _>>()
                .map(stduritemplate::Value::List),
            Value::Object(object_values) => object_values
                .iter()
                .map(|(field_name, field_value)| {
                    Self::template_value(field_value).map(|template_value| (field_name.clone(), template_value))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(stduritemplate::Value::Map),
        }
    }
}

#[derive(Debug)]
pub struct McpClient {
    server_config: McpServerConfig,
    endpoint_approval: McpEndpointApproval,
    initialized: AtomicBool,
    resource_locator_cache: Mutex<HashMap<String, McpResourceLocator>>,
    http_agent: ureq::Agent,
}

pub trait McpClientBackend: fmt::Debug + Send + Sync {
    fn list_tools(&self) -> Result<McpServerLock, McpError>;

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError>;

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError>;

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError>;
}

pub trait McpClientFactory: fmt::Debug + Send + Sync {
    fn approve_endpoint(&self, server_name: &str, endpoint: &str) -> Result<McpEndpointApproval, McpError> {
        self.validate_endpoint(server_name, endpoint)?;

        Ok(McpEndpointApproval::unrestricted(server_name, endpoint))
    }

    fn validate_endpoint(&self, _server_name: &str, _endpoint: &str) -> Result<(), McpError> {
        Ok(())
    }

    fn client_for_approved_config(
        &self,
        server_config: McpServerConfig,
        endpoint_approval: &McpEndpointApproval,
    ) -> Result<Arc<dyn McpClientBackend>, McpError> {
        endpoint_approval.validate_for_dispatch(&server_config)?;

        self.client_for_config(server_config)
    }

    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError>;
}

#[derive(Debug)]
pub struct McpClientRequestScope<'factory> {
    client_factory: &'factory dyn McpClientFactory,
    endpoint_approvals: HashMap<(String, String), McpEndpointApproval>,
}

impl<'factory> McpClientRequestScope<'factory> {
    pub fn from_workflow(
        client_factory: &'factory dyn McpClientFactory,
        workflow: &Workflow,
        evaluation_context: &EvaluationContext,
    ) -> Result<Self, McpError> {
        let mut request_scope = Self {
            client_factory,
            endpoint_approvals: HashMap::new(),
        };

        for declaration in workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let (server_name, endpoint) = McpServerConfig::resolve_endpoint_from_declaration(mcp_server_declaration, evaluation_context)?;
            request_scope.store_endpoint_approval(&server_name, &endpoint)?;
        }

        Ok(request_scope)
    }

    pub fn from_server_configs(
        client_factory: &'factory dyn McpClientFactory,
        server_configs: &[McpServerConfig],
    ) -> Result<Self, McpError> {
        let mut request_scope = Self {
            client_factory,
            endpoint_approvals: HashMap::new(),
        };

        for server_config in server_configs {
            request_scope.store_endpoint_approval(&server_config.name, &server_config.endpoint)?;
        }

        Ok(request_scope)
    }

    fn store_endpoint_approval(&mut self, server_name: &str, endpoint: &str) -> Result<(), McpError> {
        let endpoint_approval = self.client_factory.approve_endpoint(server_name, endpoint)?;
        self.endpoint_approvals
            .insert((server_name.to_string(), endpoint.to_string()), endpoint_approval);

        Ok(())
    }

    fn endpoint_approval(&self, server_config: &McpServerConfig) -> Result<&McpEndpointApproval, McpError> {
        self.endpoint_approvals
            .get(&(server_config.name.clone(), server_config.endpoint.clone()))
            .ok_or_else(|| McpError::EndpointNotApproved {
                server_name: server_config.name.clone(),
            })
    }
}

impl McpClientFactory for McpClientRequestScope<'_> {
    fn approve_endpoint(&self, server_name: &str, endpoint: &str) -> Result<McpEndpointApproval, McpError> {
        self.endpoint_approvals
            .get(&(server_name.to_string(), endpoint.to_string()))
            .cloned()
            .ok_or_else(|| McpError::EndpointNotApproved {
                server_name: server_name.to_string(),
            })
    }

    fn validate_endpoint(&self, server_name: &str, endpoint: &str) -> Result<(), McpError> {
        self.approve_endpoint(server_name, endpoint).map(|_approval| ())
    }

    fn client_for_approved_config(
        &self,
        server_config: McpServerConfig,
        endpoint_approval: &McpEndpointApproval,
    ) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let scoped_approval = self.endpoint_approval(&server_config)?;

        endpoint_approval.validate_for_dispatch(&server_config)?;

        self.client_factory.client_for_approved_config(server_config, scoped_approval)
    }

    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let endpoint_approval = self.endpoint_approval(&server_config)?;

        self.client_factory.client_for_approved_config(server_config, endpoint_approval)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HttpMcpClientFactory;

impl HttpMcpClientFactory {
    #[must_use]
    pub fn for_network_policy(network_policy: McpNetworkPolicy) -> PolicyMcpClientFactory {
        PolicyMcpClientFactory::new(network_policy)
    }
}

#[derive(Debug, Clone)]
pub struct PolicyMcpClientFactory {
    network_policy: McpNetworkPolicy,
    dns_resolver: Arc<dyn McpDnsResolver>,
    blocking_executor: Arc<McpBlockingExecutor>,
}

impl PolicyMcpClientFactory {
    #[must_use]
    pub fn new(network_policy: McpNetworkPolicy) -> Self {
        Self::with_dns_resolver(network_policy, Arc::new(SystemMcpDnsResolver))
    }

    pub fn with_dns_resolver(network_policy: McpNetworkPolicy, dns_resolver: Arc<dyn McpDnsResolver>) -> Self {
        Self {
            network_policy,
            dns_resolver,
            blocking_executor: McpBlockingExecutor::shared(),
        }
    }

    #[must_use]
    pub const fn network_policy(&self) -> McpNetworkPolicy {
        self.network_policy
    }

    fn approval_for_config(&self, server_config: &McpServerConfig) -> Result<McpEndpointApproval, McpError> {
        let network_policy = self.network_policy;
        let dns_resolver = Arc::clone(&self.dns_resolver);
        let server_name = server_config.name.clone();
        let endpoint = server_config.endpoint.clone();

        self.blocking_executor.execute(McpBlockingOperation::ApproveEndpoint, move || {
            network_policy.approve_endpoint(&server_name, &endpoint, dns_resolver.as_ref())
        })
    }
}

impl McpClient {
    fn new(server_config: McpServerConfig, endpoint_approval: &McpEndpointApproval) -> Self {
        let http_agent = endpoint_approval.http_agent();

        Self {
            server_config,
            endpoint_approval: endpoint_approval.clone(),
            initialized: AtomicBool::new(false),
            resource_locator_cache: Mutex::new(HashMap::new()),
            http_agent,
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
        let resource_locators = self.fetch_resource_locators()?;
        let resource_names = resource_locators.keys().cloned().collect::<Vec<_>>();
        self.resource_locator_cache
            .lock()
            .expect("resource locator cache lock poisoned")
            .extend(resource_locators);

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
        let resource_locators = self.fetch_resource_locators()?;
        let static_resources = resource_locators
            .iter()
            .filter_map(|(resource_name, resource_locator)| match resource_locator {
                McpResourceLocator::Static(resource_uri) => Some((resource_name.clone(), resource_uri.clone())),
                McpResourceLocator::Template(_) => None,
            })
            .collect();
        self.resource_locator_cache
            .lock()
            .expect("resource locator cache lock poisoned")
            .extend(resource_locators);

        Ok(static_resources)
    }

    fn fetch_resource_locators(&self) -> Result<BTreeMap<String, McpResourceLocator>, McpError> {
        let list_resources_result = self
            .request_result::<ListResourcesResult, _>(ListResourcesRequest::method_value(), &mcp_request!(ListResourcesRequest, 2, None))?;
        let list_resource_templates_result = self.request_result::<ListResourceTemplatesResult, _>(
            ListResourceTemplatesRequest::method_value(),
            &mcp_request!(ListResourceTemplatesRequest, 3, None),
        )?;
        let mut resource_locators = list_resources_result
            .resources
            .into_iter()
            .map(|resource| (resource.name, McpResourceLocator::Static(resource.uri)))
            .collect::<BTreeMap<_, _>>();

        for resource_template in list_resource_templates_result.resource_templates {
            let resource_name = resource_template.name.clone();
            let resource_locator = McpResourceTemplate::from_schema(resource_template).map_err(|message| McpError::InvalidResponse {
                server_name: self.server_config.name.clone(),
                method: ListResourceTemplatesRequest::method_value().to_string(),
                message,
            })?;

            resource_locators.insert(resource_name, McpResourceLocator::Template(resource_locator));
        }

        log::info!(
            "MCP resources/list completed: server={}, resources={}",
            self.server_config.name,
            resource_locators.len()
        );

        Ok(resource_locators)
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

        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            let safe_message = result
                .get("content")
                .and_then(Value::as_array)
                .and_then(|content_items| {
                    content_items.iter().find_map(|content_item| {
                        (content_item.get("type").and_then(Value::as_str) == Some("text"))
                            .then(|| content_item.get("text").and_then(Value::as_str))
                            .flatten()
                    })
                })
                .unwrap_or("MCP tool reported an error")
                .to_string();

            return Err(McpError::ToolCallFailed {
                server_name: self.server_config.name.clone(),
                tool_name: tool_name.to_string(),
                message: safe_message,
                detail: result,
            });
        }

        log::info!("MCP tools/call completed: server={}, tool={tool_name}", self.server_config.name);

        Ok(result)
    }

    pub fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError> {
        let resource_locator = self.resolve_resource_locator(resource_name)?;
        let resource_uri = resource_locator.resolve_uri(&arguments, &self.server_config.name, resource_name)?;

        log::debug!("MCP resources/read: server={}, resource={resource_name}", self.server_config.name);
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

    fn resolve_resource_locator(&self, resource_name: &str) -> Result<McpResourceLocator, McpError> {
        {
            let cache = self.resource_locator_cache.lock().expect("resource locator cache lock poisoned");

            if let Some(resource_locator) = cache.get(resource_name) {
                return Ok(resource_locator.clone());
            }
        }

        self.ensure_initialized()?;
        let resource_locators = self.fetch_resource_locators()?;
        let resource_locator = resource_locators.get(resource_name).cloned().ok_or_else(|| McpError::Rpc {
            server_name: self.server_config.name.clone(),
            method: ListResourcesRequest::method_value().to_string(),
            message: format!("resource `{resource_name}` not found in server's resource list"),
        })?;

        self.resource_locator_cache
            .lock()
            .expect("resource locator cache lock poisoned")
            .extend(resource_locators);

        Ok(resource_locator)
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
            Ok(JsonrpcResponse::ResultResponse(result_response)) => {
                to_json_value(&result_response.result).map_err(|error| McpError::InvalidResponse {
                    server_name: self.server_config.name.clone(),
                    method: method.to_string(),
                    message: error.to_string(),
                })
            }
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
        authorize_current_mcp_http_dispatch()?;
        self.endpoint_approval.validate_for_dispatch(&self.server_config)?;

        let request = self.http_post_request();

        let mut response = request.send_json(body).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        let response_body = response
            .body_mut()
            .with_config()
            .limit(MCP_HTTP_MAX_RESPONSE_BODY_BYTES)
            .read_to_string()
            .map_err(|error| McpError::Http {
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
        let mut request = self.http_agent.post(&self.server_config.endpoint);

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

#[derive(Debug)]
struct BoundedMcpClient {
    client: Arc<McpClient>,
    blocking_executor: Arc<McpBlockingExecutor>,
}

impl BoundedMcpClient {
    fn execute<ResultType, Operation>(&self, operation: McpBlockingOperation, operation_function: Operation) -> Result<ResultType, McpError>
    where
        ResultType: Send + 'static,
        Operation: FnOnce(Arc<McpClient>) -> Result<ResultType, McpError> + Send + 'static,
    {
        let client = Arc::clone(&self.client);

        self.blocking_executor.execute(operation, move || operation_function(client))
    }
}

impl McpClientBackend for BoundedMcpClient {
    fn list_tools(&self) -> Result<McpServerLock, McpError> {
        self.execute(McpBlockingOperation::ListTools, |client| client.list_tools())
    }

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        let tool_name = tool_name.to_string();

        self.execute(McpBlockingOperation::CallTool, move |client| {
            client.call_tool(&tool_name, arguments)
        })
    }

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError> {
        let resource_name = resource_name.to_string();

        self.execute(McpBlockingOperation::ReadResource, move |client| {
            client.read_resource(&resource_name, arguments)
        })
    }

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError> {
        let prompt_name = prompt_name.to_string();

        self.execute(McpBlockingOperation::GetPrompt, move |client| {
            client.get_prompt(&prompt_name, arguments)
        })
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

impl McpClientFactory for PolicyMcpClientFactory {
    fn approve_endpoint(&self, server_name: &str, endpoint: &str) -> Result<McpEndpointApproval, McpError> {
        let server_config = McpServerConfig {
            name: server_name.to_string(),
            endpoint: endpoint.to_string(),
            headers: BTreeMap::new(),
        };

        self.approval_for_config(&server_config)
    }

    fn validate_endpoint(&self, server_name: &str, endpoint: &str) -> Result<(), McpError> {
        self.approve_endpoint(server_name, endpoint).map(|_approval| ())
    }

    fn client_for_approved_config(
        &self,
        server_config: McpServerConfig,
        endpoint_approval: &McpEndpointApproval,
    ) -> Result<Arc<dyn McpClientBackend>, McpError> {
        endpoint_approval.validate_for_dispatch(&server_config)?;

        if !endpoint_approval.is_policy_approved() {
            return Err(McpError::EndpointApprovalMismatch {
                server_name: server_config.name,
            });
        }

        let client = Arc::new(McpClient::new(server_config, endpoint_approval));
        let initialization_client = Arc::clone(&client);
        self.blocking_executor
            .execute(McpBlockingOperation::Initialize, move || initialization_client.ensure_initialized())?;

        Ok(Arc::new(BoundedMcpClient {
            client,
            blocking_executor: Arc::clone(&self.blocking_executor),
        }))
    }

    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let endpoint_approval = self.approval_for_config(&server_config)?;

        self.client_for_approved_config(server_config, &endpoint_approval)
    }
}

impl McpClientFactory for HttpMcpClientFactory {
    fn validate_endpoint(&self, server_name: &str, endpoint: &str) -> Result<(), McpError> {
        PolicyMcpClientFactory::new(McpNetworkPolicy::Disabled).validate_endpoint(server_name, endpoint)
    }

    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        PolicyMcpClientFactory::new(McpNetworkPolicy::Disabled).client_for_config(server_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicUsize;

    use std::thread;
    use std::time::{Duration, Instant};

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

    #[derive(Debug)]
    struct DropSignal {
        dropped_sender: Option<std::sync::mpsc::Sender<()>>,
    }

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(dropped_sender) = self.dropped_sender.take() {
                let _send_result = dropped_sender.send(());
            }
        }
    }

    #[derive(Debug)]
    struct CountingMcpServer {
        endpoint: String,
        received_requests: Arc<Mutex<Vec<TestHttpRequest>>>,
        stop: Arc<AtomicBool>,
        server_thread: Option<thread::JoinHandle<()>>,
    }

    impl CountingMcpServer {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("counting MCP listener should bind");
            listener
                .set_nonblocking(true)
                .expect("counting MCP listener should become nonblocking");
            let endpoint = format!(
                "http://{}",
                listener.local_addr().expect("counting MCP listener address should exist")
            );
            let received_requests = Arc::new(Mutex::new(Vec::new()));
            let thread_received_requests = Arc::clone(&received_requests);
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let server_thread = thread::spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _peer_address)) => {
                            let request = read_test_http_request(&stream).expect("counting MCP request should read");
                            let response = http_json_response(json!({
                                "jsonrpc": "2.0",
                                "id": request.body.get("id").cloned().unwrap_or_else(|| json!(1)),
                                "result": {
                                    "content": [{ "type": "text", "text": "{}" }],
                                    "isError": false
                                }
                            }));
                            thread_received_requests
                                .lock()
                                .expect("counting MCP request lock should not poison")
                                .push(request);
                            stream.write_all(response.as_bytes()).expect("counting MCP response should write");
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("counting MCP listener failed: {error}"),
                    }
                }
            });

            Self {
                endpoint,
                received_requests,
                stop,
                server_thread: Some(server_thread),
            }
        }

        fn endpoint(&self) -> String {
            self.endpoint.clone()
        }

        fn request_count(&self) -> usize {
            self.received_requests
                .lock()
                .expect("counting MCP request lock should not poison")
                .len()
        }
    }

    impl Drop for CountingMcpServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);

            if let Some(server_thread) = self.server_thread.take() {
                server_thread.join().expect("counting MCP server thread should finish");
            }
        }
    }

    #[test]
    fn sends_json_and_event_stream_accept_header_for_all_mcp_requests() {
        let (endpoint, received_requests, server_thread) = spawn_mcp_server(TestResponseKind::Json);
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        };
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted test endpoint should be approved");
        let client = McpClient::new(server_config, &endpoint_approval);

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
                ListResourceTemplatesRequest::method_value(),
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
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        };
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted test endpoint should be approved");
        let client = McpClient::new(server_config, &endpoint_approval);

        let server_lock = client.list_tools().expect("tools should list from event stream responses");

        server_thread.join().expect("server thread should finish");
        assert!(server_lock.tools.contains_key("echo"));
        assert_eq!(server_lock.resources, vec!["project-readme".to_string()]);
        assert_eq!(server_lock.prompts, vec!["summarize".to_string()]);
    }

    #[test]
    fn returns_typed_error_for_mcp_tool_error_result() {
        let error_detail = json!({
            "content": [
                { "type": "text", "text": "remote validation failed" },
                { "type": "resource_link", "uri": "private://diagnostic/7" }
            ],
            "isError": true
        });
        let response_detail = error_detail.clone();
        let (endpoint, received_requests, server_thread) = spawn_scripted_mcp_server(1, move |_request| response_detail.clone());
        let client = initialized_client(endpoint);
        let error = client
            .call_tool("update_record", json!({ "record_id": 7 }))
            .expect_err("isError tool result should be a typed MCP failure");
        let McpError::ToolCallFailed {
            server_name,
            tool_name,
            message,
            detail,
        } = error
        else {
            panic!("expected typed MCP tool failure");
        };

        server_thread.join().expect("server thread should finish");
        assert_eq!(server_name, "local");
        assert_eq!(tool_name, "update_record");
        assert_eq!(message, "remote validation failed");
        assert_eq!(detail, error_detail);
        assert_eq!(
            received_requests.lock().expect("received requests lock should not poison")[0]
                .body
                .pointer("/params/arguments/record_id"),
            Some(&json!(7))
        );
    }

    #[test]
    fn expands_advertised_resource_template_into_read_uri() {
        let (endpoint, received_requests, server_thread) =
            spawn_scripted_mcp_server(3, |request| match request.get("method").and_then(Value::as_str) {
                Some(method) if method == ListResourcesRequest::method_value() => json!({
                    "resources": [{
                        "name": "project-readme",
                        "uri": "file:///static/README.md"
                    }]
                }),
                Some(method) if method == ListResourceTemplatesRequest::method_value() => json!({
                    "resourceTemplates": [{
                        "name": "project-readme",
                        "uriTemplate": "file:///workspaces/{workspace_id}/README.md{?section}"
                    }]
                }),
                Some(method) if method == ReadResourceRequest::method_value() => json!({
                    "contents": [{
                        "uri": request.pointer("/params/uri").cloned().unwrap_or(Value::Null),
                        "text": "readme"
                    }]
                }),
                _ => json!({}),
            });
        let client = initialized_client(endpoint);

        client
            .read_resource(
                "project-readme",
                json!({
                    "workspace_id": "space one",
                    "section": "setup"
                }),
            )
            .expect("advertised resource template should expand");

        server_thread.join().expect("server thread should finish");
        let received_requests = received_requests.lock().expect("received requests lock should not poison");
        let received_methods = received_requests
            .iter()
            .map(|request| request.body.get("method").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            received_methods,
            vec![
                Some(ListResourcesRequest::method_value()),
                Some(ListResourceTemplatesRequest::method_value()),
                Some(ReadResourceRequest::method_value())
            ]
        );
        assert_eq!(
            received_requests[2].body.pointer("/params/uri"),
            Some(&json!("file:///workspaces/space%20one/README.md?section=setup"))
        );
    }

    #[test]
    fn rejects_bindings_for_static_resource_before_read_request() {
        let (endpoint, received_requests, server_thread) =
            spawn_scripted_mcp_server(2, |request| match request.get("method").and_then(Value::as_str) {
                Some(method) if method == ListResourcesRequest::method_value() => json!({
                    "resources": [{
                        "name": "project-readme",
                        "uri": "file:///README.md"
                    }]
                }),
                Some(method) if method == ListResourceTemplatesRequest::method_value() => json!({
                    "resourceTemplates": []
                }),
                _ => json!({}),
            });
        let client = initialized_client(endpoint);
        let error = client
            .read_resource("project-readme", json!({ "section": "setup" }))
            .expect_err("static resource should reject discarded arguments");

        server_thread.join().expect("server thread should finish");
        assert!(matches!(error, McpError::InvalidResourceArguments { .. }));
        let received_requests = received_requests.lock().expect("received requests lock should not poison");
        assert_eq!(received_requests.len(), 2);
        assert!(received_requests
            .iter()
            .all(|request| request.body.get("method").and_then(Value::as_str) != Some(ReadResourceRequest::method_value())));
    }

    #[test]
    fn validates_resource_template_argument_names_before_read_request() {
        let (endpoint, received_requests, server_thread) =
            spawn_scripted_mcp_server(2, |request| match request.get("method").and_then(Value::as_str) {
                Some(method) if method == ListResourcesRequest::method_value() => json!({
                    "resources": []
                }),
                Some(method) if method == ListResourceTemplatesRequest::method_value() => json!({
                    "resourceTemplates": [{
                        "name": "project-readme",
                        "uriTemplate": "file:///workspaces/{workspace_id}/README.md{?section}"
                    }]
                }),
                _ => json!({}),
            });
        let client = initialized_client(endpoint);
        let error = client
            .read_resource("project-readme", json!({ "unexpected": "value" }))
            .expect_err("resource template should validate missing and unknown arguments");

        server_thread.join().expect("server thread should finish");
        let McpError::InvalidResourceArguments { message, .. } = error else {
            panic!("expected invalid resource arguments");
        };

        assert!(message.contains("missing required arguments: workspace_id"));
        assert!(message.contains("unexpected arguments: unexpected"));
        let received_requests = received_requests.lock().expect("received requests lock should not poison");
        assert_eq!(received_requests.len(), 2);
        assert!(received_requests
            .iter()
            .all(|request| request.body.get("method").and_then(Value::as_str) != Some(ReadResourceRequest::method_value())));
    }

    #[test]
    fn slow_response_headers_are_bounded() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("slow test listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("slow listener address should exist"));
        let (request_received_sender, request_received_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let server_thread = thread::spawn(move || {
            let (stream, _peer_address) = listener.accept().expect("slow test connection should open");
            read_test_http_request(&stream).expect("slow test request should read");
            request_received_sender.send(()).expect("slow request arrival should be observable");
            release_receiver.recv().expect("slow test server should be released");
        });
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        };
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted slow endpoint should be approved");
        let http_agent = endpoint_approval.http_agent_with_timeout(Duration::from_millis(100));
        let client = initialized_client_with_http_agent(server_config, endpoint_approval, http_agent);
        let started_at = Instant::now();
        let error = client
            .call_tool("slow_tool", json!({}))
            .expect_err("slow response should exceed the HTTP bound");

        request_received_receiver.recv().expect("slow server should observe the request");
        let McpError::Http { message, .. } = error else {
            panic!("slow response should return an HTTP error");
        };
        assert!(message.contains("timeout"));
        assert!(started_at.elapsed() < Duration::from_secs(2));
        release_sender.send(()).expect("slow server should be released");
        server_thread.join().expect("slow server thread should finish");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_caller_finishes_within_the_underlying_request_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("cancellation test listener should bind");
        let endpoint = format!(
            "http://{}",
            listener.local_addr().expect("cancellation listener address should exist")
        );
        let (request_received_sender, request_received_receiver) = tokio::sync::oneshot::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let server_thread = thread::spawn(move || {
            let (stream, _peer_address) = listener.accept().expect("cancellation test connection should open");
            read_test_http_request(&stream).expect("cancellation test request should read");
            request_received_sender
                .send(())
                .expect("cancellation request arrival should be observable");
            release_receiver.recv().expect("cancellation test server should be released");
        });
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        };
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted cancellation endpoint should be approved");
        let http_agent = endpoint_approval.http_agent_with_timeout(Duration::from_millis(100));
        let bounded_client = Arc::new(BoundedMcpClient {
            client: Arc::new(initialized_client_with_http_agent(server_config, endpoint_approval, http_agent)),
            blocking_executor: McpBlockingExecutor::shared(),
        });
        let caller_client = Arc::clone(&bounded_client);
        let caller_task = tokio::spawn(async move { caller_client.call_tool("slow_tool", json!({})) });

        request_received_receiver
            .await
            .expect("cancellation server should observe the request");
        let cancellation_started_at = Instant::now();
        caller_task.abort();
        let caller_result = tokio::time::timeout(Duration::from_secs(2), caller_task)
            .await
            .expect("cancelled caller should finish within the HTTP timeout");
        release_sender.send(()).expect("cancellation test server should be released");
        server_thread.join().expect("cancellation test server thread should finish");

        match caller_result {
            Ok(Err(McpError::Http { .. })) => {}
            Err(join_error) if join_error.is_cancelled() => {}
            other_result => panic!("unexpected cancelled caller result: {other_result:?}"),
        }
        assert!(cancellation_started_at.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn http_job_with_insufficient_caller_lifetime_is_rejected_before_side_effects() {
        let blocking_executor = McpBlockingExecutor::with_limits(1, 1);
        let side_effect_count = Arc::new(AtomicUsize::new(0));
        let operation_side_effect_count = Arc::clone(&side_effect_count);
        let error = blocking_executor
            .execute_with_timeout(McpBlockingOperation::CallTool, Duration::from_secs(1), move || {
                operation_side_effect_count.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
            .expect_err("HTTP job should require the complete mandatory request bound");

        assert!(matches!(
            error,
            McpError::BlockingOperationDeadlineInsufficient {
                operation: McpBlockingOperation::CallTool
            }
        ));
        assert_eq!(side_effect_count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn timed_out_credentialed_queued_call_is_discarded_without_http_side_effects() {
        let counting_server = CountingMcpServer::spawn();
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer abandoned-secret".to_string());
        let server_config = McpServerConfig {
            name: "mutating".to_string(),
            endpoint: counting_server.endpoint(),
            headers,
        };
        let mut endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted mutating endpoint should be approved");
        endpoint_approval.expire_for_test();
        let http_agent = endpoint_approval.http_agent();
        let mutating_client = Arc::new(initialized_client_with_http_agent(server_config, endpoint_approval, http_agent));
        let blocking_executor = Arc::new(McpBlockingExecutor::with_limits(1, 1));
        let (first_started_sender, first_started_receiver) = std::sync::mpsc::channel();
        let (first_release_sender, first_release_receiver) = std::sync::mpsc::channel();
        let first_executor = Arc::clone(&blocking_executor);
        let first_thread = thread::spawn(move || {
            first_executor.execute(McpBlockingOperation::CallTool, move || {
                first_started_sender
                    .send(())
                    .expect("first blocking operation start should be observable");
                first_release_receiver.recv().expect("first blocking operation should be released");
                Ok(())
            })
        });
        first_started_receiver.recv().expect("first blocking operation should start");

        let (dropped_sender, dropped_receiver) = std::sync::mpsc::channel();
        let queued_drop_signal = DropSignal {
            dropped_sender: Some(dropped_sender),
        };
        let queued_executor = Arc::clone(&blocking_executor);
        let queued_client = mutating_client;
        let queued_thread = thread::spawn(move || {
            queued_executor.execute_with_timeout(McpBlockingOperation::CallTool, Duration::from_millis(50), move || {
                let _queued_drop_signal = queued_drop_signal;

                queued_client.call_tool("mutate_record", json!({ "record_id": 7 }))
            })
        });
        let queue_wait_started_at = Instant::now();

        while blocking_executor.queued_job_count() != 1 {
            assert!(
                queue_wait_started_at.elapsed() < Duration::from_secs(1),
                "credentialed operation should enter the bounded queue"
            );
            thread::yield_now();
        }

        let queued_error = queued_thread
            .join()
            .expect("queued caller thread should finish")
            .expect_err("queued caller should time out before the first operation is released");
        assert!(matches!(
            queued_error,
            McpError::BlockingOperationTimedOut {
                operation: McpBlockingOperation::CallTool
            }
        ));

        first_release_sender.send(()).expect("first blocking operation should be released");
        first_thread
            .join()
            .expect("first caller thread should finish")
            .expect("first blocking operation should succeed");
        dropped_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("discarded credentialed closure should be dropped");

        assert_eq!(counting_server.request_count(), 0);
    }

    #[test]
    fn expired_approval_is_revalidated_immediately_before_http_dispatch() {
        let counting_server = CountingMcpServer::spawn();
        let server_config = McpServerConfig {
            name: "expired".to_string(),
            endpoint: counting_server.endpoint(),
            headers: BTreeMap::new(),
        };
        let mut endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted endpoint should be approved");
        endpoint_approval.expire_for_test();
        let http_agent = endpoint_approval.http_agent();
        let bounded_client = BoundedMcpClient {
            client: Arc::new(initialized_client_with_http_agent(server_config, endpoint_approval, http_agent)),
            blocking_executor: Arc::new(McpBlockingExecutor::with_limits(1, 1)),
        };
        let error = bounded_client
            .call_tool("mutate_record", json!({ "record_id": 7 }))
            .expect_err("expired approval should fail before HTTP dispatch");

        assert!(matches!(error, McpError::EndpointApprovalExpired { .. }));
        assert_eq!(counting_server.request_count(), 0);
    }

    #[test]
    fn oversized_response_body_is_rejected_at_the_configured_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("oversized test listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("oversized listener address should exist"));
        let server_thread = thread::spawn(move || {
            let (mut stream, _peer_address) = listener.accept().expect("oversized test connection should open");
            read_test_http_request(&stream).expect("oversized test request should read");
            let response_body_length = usize::try_from(MCP_HTTP_MAX_RESPONSE_BODY_BYTES).expect("response limit should fit usize") + 1;
            let response_body = vec![b'x'; response_body_length];
            let response_headers = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n",
                response_body.len()
            );
            stream
                .write_all(response_headers.as_bytes())
                .expect("oversized response headers should write");
            stream.write_all(&response_body).expect("oversized response body should write");
        });
        let client = initialized_client(endpoint);
        let error = client
            .call_tool("oversized_tool", json!({}))
            .expect_err("oversized response should exceed the body limit");
        let McpError::Http { message, .. } = error else {
            panic!("oversized response should return an HTTP error");
        };

        assert!(message.contains("limit"));
        server_thread.join().expect("oversized server thread should finish");
    }

    fn initialized_client(endpoint: String) -> McpClient {
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint,
            headers: BTreeMap::new(),
        };
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint(&server_config.name, &server_config.endpoint, &SystemMcpDnsResolver)
            .expect("trusted test endpoint should be approved");

        let http_agent = endpoint_approval.http_agent();

        initialized_client_with_http_agent(server_config, endpoint_approval, http_agent)
    }

    fn initialized_client_with_http_agent(
        server_config: McpServerConfig,
        endpoint_approval: McpEndpointApproval,
        http_agent: ureq::Agent,
    ) -> McpClient {
        McpClient {
            server_config,
            endpoint_approval,
            initialized: AtomicBool::new(true),
            resource_locator_cache: Mutex::new(HashMap::new()),
            http_agent,
        }
    }

    fn spawn_scripted_mcp_server<ResponseFactory>(
        request_count: usize,
        response_factory: ResponseFactory,
    ) -> (String, Arc<Mutex<Vec<TestHttpRequest>>>, thread::JoinHandle<()>)
    where
        ResponseFactory: Fn(&Value) -> Value + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let received_requests = Arc::new(Mutex::new(Vec::new()));
        let thread_received_requests = Arc::clone(&received_requests);
        let server_thread = thread::spawn(move || {
            for incoming_stream in listener.incoming().take(request_count) {
                let mut stream = incoming_stream.expect("incoming stream should open");
                let request = read_test_http_request(&stream).expect("HTTP request should read");
                let result = response_factory(&request.body);
                let response = http_json_response(json!({
                    "jsonrpc": "2.0",
                    "id": request.body.get("id").cloned().unwrap_or_else(|| json!(1)),
                    "result": result,
                }));

                thread_received_requests
                    .lock()
                    .expect("received requests lock should not poison")
                    .push(request);
                stream.write_all(response.as_bytes()).expect("HTTP response should write");
            }
        });

        (endpoint, received_requests, server_thread)
    }

    fn spawn_mcp_server(response_kind: TestResponseKind) -> (String, Arc<Mutex<Vec<TestHttpRequest>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let received_requests = Arc::new(Mutex::new(Vec::new()));
        let thread_received_requests = Arc::clone(&received_requests);
        let server_thread = thread::spawn(move || {
            for incoming_stream in listener.incoming().take(6) {
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
            method if method == ListResourceTemplatesRequest::method_value() => json!({
                "resourceTemplates": []
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
    approved_endpoints: Arc<HashMap<String, String>>,
}

impl McpClientPool {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            approved_endpoints: Arc::new(HashMap::new()),
        }
    }

    pub fn from_server_configs(configs: impl IntoIterator<Item = McpServerConfig>) -> Result<Self, McpError> {
        Self::from_server_configs_with_factory(configs, &HttpMcpClientFactory)
    }

    pub fn from_server_configs_with_factory(
        configs: impl IntoIterator<Item = McpServerConfig>,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let configs = configs.into_iter().collect::<Vec<_>>();
        let request_scope = McpClientRequestScope::from_server_configs(client_factory, &configs)?;
        let mut clients = HashMap::new();
        let mut approved_endpoints = HashMap::new();
        for server_config in configs {
            log::debug!("initializing MCP client pool for server: {}", server_config.name);
            let client = request_scope.client_for_config(server_config.clone())?;
            approved_endpoints.insert(server_config.name.clone(), server_config.endpoint.clone());
            clients.insert(server_config.name, client);
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
            approved_endpoints: Arc::new(approved_endpoints),
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
        let mut approved_endpoints = HashMap::new();
        let mut evaluation_context = evaluation_context.clone();
        evaluation_context.evaluate_available_workflow_dynamic_bindings(workflow);
        let request_scope = McpClientRequestScope::from_workflow(client_factory, workflow, &evaluation_context)?;

        for declaration in workflow.declarations() {
            let superwire_types::ast::Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration_with_endpoint_validator(
                mcp_server_declaration,
                &evaluation_context,
                |server_name, endpoint| request_scope.validate_endpoint(server_name, endpoint),
            )?;
            log::debug!("initializing MCP client pool for runtime server: {}", server_config.name);
            let client = request_scope.client_for_config(server_config.clone())?;
            approved_endpoints.insert(server_config.name.clone(), server_config.endpoint.clone());
            clients.insert(server_config.name, client);
        }

        Ok(Self {
            clients: Arc::new(Mutex::new(clients)),
            approved_endpoints: Arc::new(approved_endpoints),
        })
    }

    pub fn from_clients(clients: impl IntoIterator<Item = (String, Arc<dyn McpClientBackend>)>) -> Self {
        Self {
            clients: Arc::new(Mutex::new(clients.into_iter().collect())),
            approved_endpoints: Arc::new(HashMap::new()),
        }
    }

    pub fn validate_endpoint(&self, server_name: &str, endpoint: &str) -> Result<(), McpError> {
        let Some(approved_endpoint) = self.approved_endpoints.get(server_name) else {
            return Ok(());
        };

        if approved_endpoint == endpoint {
            return Ok(());
        }

        Err(McpError::InvalidPropertyEvaluation {
            server_name: server_name.to_string(),
            property_name: superwire_types::ast::McpServerPropertyName::Endpoint.as_str().to_string(),
            reason: "endpoint changed after MCP client initialization".to_string(),
        })
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
