use async_trait::async_trait;
use engine_ai_agent::{
    AgentError, Context, Executable, LoopExecutor, OllamaProvider, OpenAIProvider, Provider, RuntimeTool, ToolDefinition, ToolError,
};
use engine_ai_core::dsl::parse_workflow;
use engine_ai_core::{
    AgentExecutionRequest, AgentExecutionResult, AgentRunner, AgentToolConfiguration, ProviderConfig, WorkflowRuntime, WorkflowRuntimeError,
};
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::fs;
use std::sync::{Arc, OnceLock, RwLock};
use thiserror::Error;

#[cfg(feature = "php-ext")]
const FFI_MODULE_NAME: &str = "engine_ai_ffi";

pub trait ToolInvocationBinding: Send + Sync {
    fn invoke_tool(&self, request: ToolInvocationRequest) -> Result<ToolInvocationResponse, FfiError>;
}

pub struct JsonToolInvocationBinding<Callback>
where
    Callback: Fn(&str) -> Result<String, FfiError> + Send + Sync,
{
    callback: Callback,
}

impl<Callback> JsonToolInvocationBinding<Callback>
where
    Callback: Fn(&str) -> Result<String, FfiError> + Send + Sync,
{
    #[must_use]
    pub fn new(callback: Callback) -> Self {
        Self { callback }
    }
}

impl<Callback> ToolInvocationBinding for JsonToolInvocationBinding<Callback>
where
    Callback: Fn(&str) -> Result<String, FfiError> + Send + Sync,
{
    fn invoke_tool(&self, request: ToolInvocationRequest) -> Result<ToolInvocationResponse, FfiError> {
        let request_json = request.to_json()?;
        let response_json = (self.callback)(&request_json)?;

        ToolInvocationResponse::from_json(&response_json)
    }
}

type SharedToolInvocationBinding = Arc<dyn ToolInvocationBinding>;

static GLOBAL_TOOL_INVOCATION_BINDING: OnceLock<RwLock<Option<SharedToolInvocationBinding>>> = OnceLock::new();

fn global_tool_invocation_binding() -> &'static RwLock<Option<SharedToolInvocationBinding>> {
    GLOBAL_TOOL_INVOCATION_BINDING.get_or_init(|| RwLock::new(None))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionRequest {
    pub workflow_file_path: String,
    #[serde(default)]
    pub workflow_input: Value,
    #[serde(default)]
    pub workflow_secrets: Value,
    #[serde(default)]
    pub custom_tools: CustomToolRegistry,
}

impl WorkflowExecutionRequest {
    pub fn from_json(request_json: &str) -> Result<Self, FfiError> {
        serde_json::from_str(request_json).map_err(FfiError::InvalidRequest)
    }

    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }

    fn load_workflow_source(&self) -> Result<String, WorkflowExecutionError> {
        fs::read_to_string(&self.workflow_file_path).map_err(|error| WorkflowExecutionError {
            code: WorkflowExecutionErrorCode::WorkflowLoadFailed,
            message: format!("failed to read workflow file `{}`: {error}", self.workflow_file_path),
            details: None,
        })
    }

    fn parse_workflow(&self, workflow_source: &str) -> Result<engine_ai_core::dsl::Workflow, WorkflowExecutionError> {
        parse_workflow(workflow_source).map_err(|parse_error| WorkflowExecutionError {
            code: WorkflowExecutionErrorCode::WorkflowLoadFailed,
            message: parse_error.render_with_source(workflow_source, &self.workflow_file_path),
            details: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CustomToolRegistry {
    #[serde(default)]
    pub definitions: Vec<CustomToolDefinition>,
}

impl CustomToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: CustomToolDefinition) {
        self.definitions.push(definition);
    }

    #[must_use]
    pub fn registered_definitions(&self) -> &[CustomToolDefinition] {
        self.definitions.as_slice()
    }

    fn runtime_registry(&self) -> Result<RuntimeToolRegistry, WorkflowExecutionError> {
        let mut definitions_by_name = HashMap::new();

        for custom_tool_definition in &self.definitions {
            let runtime_tool_definition = custom_tool_definition.runtime_definition()?;

            if definitions_by_name
                .insert(runtime_tool_definition.name.clone(), runtime_tool_definition)
                .is_some()
            {
                return Err(WorkflowExecutionError {
                    code: WorkflowExecutionErrorCode::ToolRegistrationFailed,
                    message: format!("duplicate custom tool definition `{}`", custom_tool_definition.name),
                    details: None,
                });
            }
        }

        Ok(RuntimeToolRegistry { definitions_by_name })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub execution_contract: CustomToolExecutionContract,
}

impl CustomToolDefinition {
    fn runtime_definition(&self) -> Result<RuntimeToolDefinition, WorkflowExecutionError> {
        let parameters_schema = serde_json::from_value::<Schema>(self.input_schema.clone()).map_err(|error| WorkflowExecutionError {
            code: WorkflowExecutionErrorCode::ToolRegistrationFailed,
            message: format!("invalid `input_schema` for tool `{}`: {error}", self.name),
            details: None,
        })?;

        Ok(RuntimeToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters_schema,
            execution_contract: self.execution_contract,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, Copy)]
#[serde(rename_all = "snake_case")]
pub enum CustomToolExecutionContract {
    #[default]
    HostCallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocationRequest {
    pub tool_name: String,
    pub tool_input: Value,
}

impl ToolInvocationRequest {
    pub fn from_json(request_json: &str) -> Result<Self, FfiError> {
        serde_json::from_str(request_json).map_err(FfiError::InvalidRequest)
    }

    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocationResponse {
    pub result: ToolInvocationResult,
}

impl ToolInvocationResponse {
    pub fn from_json(response_json: &str) -> Result<Self, FfiError> {
        serde_json::from_str(response_json).map_err(FfiError::InvalidRequest)
    }

    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolInvocationResult {
    Succeeded { tool_output: Value },
    Failed { error: WorkflowExecutionError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionResponse {
    pub result: WorkflowExecutionResult,
}

impl WorkflowExecutionResponse {
    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }

    fn success(workflow_output: Value) -> Self {
        Self {
            result: WorkflowExecutionResult::Succeeded { workflow_output },
        }
    }

    fn failure(error: WorkflowExecutionError) -> Self {
        Self {
            result: WorkflowExecutionResult::Failed { error },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkflowExecutionResult {
    Succeeded { workflow_output: Value },
    Failed { error: WorkflowExecutionError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionError {
    pub code: WorkflowExecutionErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl WorkflowExecutionError {
    pub fn runtime_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::RuntimeUnavailable,
            message: message.into(),
            details: None,
        }
    }

    fn from_runtime_error(runtime_error: WorkflowRuntimeError) -> Self {
        match runtime_error {
            WorkflowRuntimeError::ParseFailed { source: _, details } => Self {
                code: WorkflowExecutionErrorCode::WorkflowLoadFailed,
                message: details,
                details: None,
            },
            WorkflowRuntimeError::InvalidWorkflow { issues } => Self {
                code: WorkflowExecutionErrorCode::WorkflowLoadFailed,
                message: issues,
                details: None,
            },
            input_or_secret_request_error @ (WorkflowRuntimeError::InputTypeMismatch { expected: _, found: _ }
            | WorkflowRuntimeError::InputValueMismatch { message: _ }
            | WorkflowRuntimeError::SecretsTypeMismatch { expected: _, found: _ }
            | WorkflowRuntimeError::SecretsValueMismatch { message: _ }) => Self {
                code: WorkflowExecutionErrorCode::InvalidRequest,
                message: input_or_secret_request_error.to_string(),
                details: None,
            },
            WorkflowRuntimeError::AgentExecutionFailed { agent_name, source } => Self::from_agent_execution_error(agent_name, *source),
            WorkflowRuntimeError::UnsupportedFeature { feature } if feature.contains("tools") || feature.contains("tool") => Self {
                code: WorkflowExecutionErrorCode::ToolRegistrationFailed,
                message: feature,
                details: None,
            },
            WorkflowRuntimeError::InvalidAgentProperty {
                agent_name,
                property,
                message,
            } if property == "tools" => Self {
                code: WorkflowExecutionErrorCode::ToolRegistrationFailed,
                message: format!("agent `{agent_name}` has invalid `tools` property: {message}"),
                details: None,
            },
            other_runtime_error => Self {
                code: WorkflowExecutionErrorCode::WorkflowExecutionFailed,
                message: other_runtime_error.to_string(),
                details: None,
            },
        }
    }

    fn from_agent_execution_error(agent_name: String, agent_error: AgentError) -> Self {
        match agent_error {
            AgentError::ExecutionFailed { error, context: _ } if error.to_string().contains("tool") => Self {
                code: WorkflowExecutionErrorCode::ToolExecutionFailed,
                message: format!("agent `{agent_name}` tool execution failed: {error}"),
                details: None,
            },
            other_agent_error => Self {
                code: WorkflowExecutionErrorCode::WorkflowExecutionFailed,
                message: format!("agent `{agent_name}` execution failed: {other_agent_error}"),
                details: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionErrorCode {
    InvalidRequest,
    WorkflowLoadFailed,
    ToolRegistrationFailed,
    ToolExecutionFailed,
    WorkflowExecutionFailed,
    RuntimeUnavailable,
}

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("failed to deserialize ffi request: {0}")]
    InvalidRequest(serde_json::Error),
    #[error("failed to serialize ffi response: {0}")]
    Serialization(serde_json::Error),
    #[error("failed to configure global tool invocation binding")]
    BindingConfiguration,
    #[error("tool invocation binding failed: {message}")]
    ToolBindingInvocation { message: String },
}

impl FfiError {
    #[cfg(feature = "php-ext")]
    fn php_error_message(&self) -> String {
        self.to_string()
    }
}

pub trait WorkflowExecutor {
    fn execute_workflow(&self, request: WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError>;
}

#[derive(Default)]
pub struct FfiInterface {
    tool_invocation_binding: Option<SharedToolInvocationBinding>,
}

impl Debug for FfiInterface {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FfiInterface")
            .field("tool_invocation_binding", &self.tool_invocation_binding.is_some())
            .finish()
    }
}

impl FfiInterface {
    #[must_use]
    pub fn with_tool_invocation_binding(tool_invocation_binding: SharedToolInvocationBinding) -> Self {
        Self {
            tool_invocation_binding: Some(tool_invocation_binding),
        }
    }

    pub fn register_global_tool_invocation_binding(tool_invocation_binding: SharedToolInvocationBinding) -> Result<(), FfiError> {
        let mut binding_guard = global_tool_invocation_binding()
            .write()
            .map_err(|_| FfiError::BindingConfiguration)?;

        *binding_guard = Some(tool_invocation_binding);

        Ok(())
    }

    pub fn clear_global_tool_invocation_binding() -> Result<(), FfiError> {
        let mut binding_guard = global_tool_invocation_binding()
            .write()
            .map_err(|_| FfiError::BindingConfiguration)?;

        *binding_guard = None;

        Ok(())
    }

    pub fn execute_workflow_from_json(&self, request_json: &str) -> Result<String, FfiError> {
        let request = WorkflowExecutionRequest::from_json(request_json)?;
        let response = self.execute_workflow(request)?;

        response.to_json()
    }

    fn active_tool_invocation_binding(&self) -> Option<SharedToolInvocationBinding> {
        if let Some(tool_invocation_binding) = &self.tool_invocation_binding {
            return Some(tool_invocation_binding.clone());
        }

        global_tool_invocation_binding().read().ok().and_then(|guard| guard.clone())
    }

    fn run_workflow_request(&self, request: WorkflowExecutionRequest) -> Result<Value, WorkflowExecutionError> {
        let workflow_source = request.load_workflow_source()?;
        let workflow = request.parse_workflow(&workflow_source)?;
        let runtime_tool_registry = request.custom_tools.runtime_registry()?;

        let workflow_runner = FfiWorkflowRunner {
            runtime_tool_registry,
            tool_invocation_binding: self.active_tool_invocation_binding(),
        };

        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| WorkflowExecutionError::runtime_unavailable(format!("failed to initialize async runtime: {error}")))?;

        tokio_runtime.block_on(async {
            let workflow_runtime = WorkflowRuntime::<Value, Value>::new(workflow).map_err(WorkflowExecutionError::from_runtime_error)?;

            workflow_runtime
                .run_with_runner_and_secrets(request.workflow_input, request.workflow_secrets, &workflow_runner)
                .await
                .map_err(WorkflowExecutionError::from_runtime_error)
        })
    }

    pub fn execute_workflow(&self, request: WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError> {
        let response = match self.run_workflow_request(request) {
            Ok(workflow_output) => WorkflowExecutionResponse::success(workflow_output),
            Err(workflow_execution_error) => WorkflowExecutionResponse::failure(workflow_execution_error),
        };

        Ok(response)
    }
}

impl WorkflowExecutor for FfiInterface {
    fn execute_workflow(&self, request: WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError> {
        FfiInterface::execute_workflow(self, request)
    }
}

#[derive(Debug, Clone)]
struct RuntimeToolRegistry {
    definitions_by_name: HashMap<String, RuntimeToolDefinition>,
}

impl RuntimeToolRegistry {
    fn runtime_tools_for_request(
        &self,
        tool_configurations: &[AgentToolConfiguration],
        tool_invocation_binding: Option<SharedToolInvocationBinding>,
    ) -> Result<Vec<Arc<dyn RuntimeTool>>, WorkflowRuntimeError> {
        let mut runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();

        for agent_tool_configuration in tool_configurations {
            let Some(runtime_tool_definition) = self.definitions_by_name.get(&agent_tool_configuration.tool_name) else {
                return Err(WorkflowRuntimeError::Other {
                    message: format!(
                        "agent requested tool `{}` but it is not registered in ffi custom_tools",
                        agent_tool_configuration.tool_name
                    ),
                });
            };

            runtime_tools.push(
                runtime_tool_definition.runtime_tool(agent_tool_configuration.bound_arguments.clone(), tool_invocation_binding.clone())?,
            );
        }

        Ok(runtime_tools)
    }
}

#[derive(Debug, Clone)]
struct RuntimeToolDefinition {
    name: String,
    description: String,
    parameters_schema: Schema,
    execution_contract: CustomToolExecutionContract,
}

impl RuntimeToolDefinition {
    fn runtime_tool(
        &self,
        bound_arguments: Map<String, Value>,
        tool_invocation_binding: Option<SharedToolInvocationBinding>,
    ) -> Result<Arc<dyn RuntimeTool>, WorkflowRuntimeError> {
        match self.execution_contract {
            CustomToolExecutionContract::HostCallback => {
                let Some(tool_invocation_binding) = tool_invocation_binding else {
                    return Err(WorkflowRuntimeError::Other {
                        message: format!(
                            "tool `{}` requires `host_callback` execution but no tool binding is registered",
                            self.name
                        ),
                    });
                };

                Ok(Arc::new(HostCallbackRuntimeTool {
                    name: self.name.clone(),
                    description: self.description.clone(),
                    parameters_schema: self.parameters_schema.clone(),
                    bound_arguments,
                    tool_invocation_binding,
                }))
            }
        }
    }
}

#[derive(Clone)]
struct HostCallbackRuntimeTool {
    name: String,
    description: String,
    parameters_schema: Schema,
    bound_arguments: Map<String, Value>,
    tool_invocation_binding: SharedToolInvocationBinding,
}

impl Debug for HostCallbackRuntimeTool {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostCallbackRuntimeTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("bound_arguments", &self.bound_arguments)
            .finish_non_exhaustive()
    }
}

impl HostCallbackRuntimeTool {
    fn merged_tool_input(&self, tool_input: Value) -> Result<Value, ToolError> {
        if self.bound_arguments.is_empty() {
            return Ok(tool_input);
        }

        let mut merged_arguments = match tool_input {
            Value::Object(object_value) => object_value,
            Value::Null => Map::new(),
            unsupported_value => {
                return Err(ToolError::new(format!(
                    "tool `{}` expects object arguments when workflow binds named tool arguments, received `{}`",
                    self.name, unsupported_value
                )));
            }
        };

        for (argument_name, argument_value) in &self.bound_arguments {
            merged_arguments.insert(argument_name.clone(), argument_value.clone());
        }

        Ok(Value::Object(merged_arguments))
    }
}

#[async_trait]
impl RuntimeTool for HostCallbackRuntimeTool {
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        Ok(ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters_schema: self.parameters_schema.clone(),
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let tool_input = self.merged_tool_input(input)?;
        let tool_invocation_request = ToolInvocationRequest {
            tool_name: self.name.clone(),
            tool_input,
        };

        let tool_invocation_response = self
            .tool_invocation_binding
            .invoke_tool(tool_invocation_request)
            .map_err(|error| ToolError::new(format!("host callback failed for tool `{}`: {error}", self.name)))?;

        match tool_invocation_response.result {
            ToolInvocationResult::Succeeded { tool_output } => Ok(tool_output),
            ToolInvocationResult::Failed { error } => {
                let mut tool_error = ToolError::new(format!("tool `{}` failed: {}", self.name, error.message));

                tool_error = tool_error.with_context("code", serde_json::json!(error.code));

                if let Some(details_value) = error.details {
                    tool_error = tool_error.with_context("details", details_value);
                }

                Err(tool_error)
            }
        }
    }
}

#[derive(Clone)]
struct FfiWorkflowRunner {
    runtime_tool_registry: RuntimeToolRegistry,
    tool_invocation_binding: Option<SharedToolInvocationBinding>,
}

impl FfiWorkflowRunner {
    async fn run_with_provider<ProviderType>(
        &self,
        provider: ProviderType,
        request: &AgentExecutionRequest,
    ) -> Result<AgentExecutionResult, WorkflowRuntimeError>
    where
        ProviderType: Provider + Send + Sync,
    {
        let runtime_tools = self
            .runtime_tool_registry
            .runtime_tools_for_request(&request.tool_configurations, self.tool_invocation_binding.clone())?;

        let executor = LoopExecutor::<ProviderType, Value>::new()
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to create loop executor for `{}`: {error}", request.agent_name),
            })?
            .with_finalize_answer_schema(request.output_schema.clone())
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to configure finalize schema for agent `{}`: {error}", request.agent_name),
            })?;

        let mut execution_context = Context::new();
        execution_context.add_user_message(request.prompt.clone());

        let execution_output = executor
            .execute(&mut execution_context, &provider, &runtime_tools, &request.config)
            .await
            .map_err(|error| WorkflowRuntimeError::AgentExecutionFailed {
                agent_name: request.agent_name.clone(),
                source: Box::new(AgentError::ExecutionFailed {
                    error,
                    context: execution_context.clone(),
                }),
            })?;

        let serialized_context = serde_json::to_value(execution_context).map_err(|error| WorkflowRuntimeError::SerializationFailed {
            context: format!("context for agent `{}`", request.agent_name),
            source: error,
        })?;

        Ok(AgentExecutionResult {
            output: execution_output,
            context: serialized_context,
        })
    }
}

#[async_trait]
impl AgentRunner for FfiWorkflowRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        match &request.provider_config {
            ProviderConfig::OpenAI(openai_provider_config) => {
                let openai_provider = OpenAIProvider::new_with_base_url(
                    openai_provider_config.endpoint.clone(),
                    openai_provider_config.api_key.clone(),
                    request.model_name.clone(),
                );

                self.run_with_provider(openai_provider, request).await
            }
            ProviderConfig::Ollama(ollama_provider_config) => {
                let ollama_provider = OllamaProvider::new(
                    ollama_provider_config.host.clone(),
                    ollama_provider_config.port,
                    request.model_name.clone(),
                );

                self.run_with_provider(ollama_provider, request).await
            }
        }
    }
}

#[cfg(feature = "php-ext")]
#[must_use]
pub fn php_extension_enabled() -> bool {
    true
}

#[cfg(feature = "php-ext")]
mod php_extension {
    use super::{FfiError, FfiInterface, JsonToolInvocationBinding, SharedToolInvocationBinding, FFI_MODULE_NAME};
    use ext_php_rs::{
        exception::PhpException,
        prelude::{ModuleBuilder, PhpResult},
        types::ZendCallable,
        wrap_function,
    };
    use serde_json::json;
    use std::sync::Arc;

    #[derive(Debug)]
    struct CapabilityInfo {
        runtime_execution_enabled: bool,
        host_callback_tools_enabled: bool,
    }

    impl CapabilityInfo {
        fn to_json(&self) -> Result<String, FfiError> {
            serde_json::to_string(&json!({
                "module": FFI_MODULE_NAME,
                "version": env!("CARGO_PKG_VERSION"),
                "capabilities": {
                    "runtime_execution_enabled": self.runtime_execution_enabled,
                    "host_callback_tools_enabled": self.host_callback_tools_enabled,
                },
            }))
            .map_err(FfiError::Serialization)
        }
    }

    impl FfiError {
        fn into_php_exception(self) -> PhpException {
            PhpException::default(self.php_error_message())
        }
    }

    fn php_tool_invocation_binding(callback_name: String) -> SharedToolInvocationBinding {
        Arc::new(JsonToolInvocationBinding::new(move |request_json| {
            let zend_callable = ZendCallable::try_from_name(&callback_name).map_err(|error| FfiError::ToolBindingInvocation {
                message: format!("failed to resolve registered callback `{callback_name}` as callable: {error}"),
            })?;

            let callback_result = zend_callable
                .try_call(vec![&request_json])
                .map_err(|error| FfiError::ToolBindingInvocation {
                    message: format!("registered callback `{callback_name}` failed while handling tool invocation: {error}"),
                })?;

            callback_result.string().ok_or_else(|| FfiError::ToolBindingInvocation {
                message: format!("registered callback `{callback_name}` must return a JSON string response"),
            })
        }))
    }

    #[ext_php_rs::php_function]
    pub fn engine_ai_register_tool_callback(callback_name: &str) -> PhpResult<bool> {
        let callback_binding = php_tool_invocation_binding(callback_name.to_string());

        FfiInterface::register_global_tool_invocation_binding(callback_binding).map_err(FfiError::into_php_exception)?;

        Ok(true)
    }

    #[ext_php_rs::php_function]
    pub fn engine_ai_clear_tool_callback() -> PhpResult<bool> {
        FfiInterface::clear_global_tool_invocation_binding().map_err(FfiError::into_php_exception)?;

        Ok(true)
    }

    #[ext_php_rs::php_function]
    pub fn engine_ai_execute_workflow(request_json: &str) -> PhpResult<String> {
        let ffi_interface = FfiInterface::default();

        ffi_interface
            .execute_workflow_from_json(request_json)
            .map_err(FfiError::into_php_exception)
    }

    #[ext_php_rs::php_function]
    pub fn engine_ai_module_info() -> PhpResult<String> {
        let capability_info = CapabilityInfo {
            runtime_execution_enabled: true,
            host_callback_tools_enabled: true,
        };

        capability_info.to_json().map_err(FfiError::into_php_exception)
    }

    #[ext_php_rs::php_module]
    pub fn get_module(module_builder: ModuleBuilder) -> ModuleBuilder {
        module_builder
            .function(wrap_function!(engine_ai_execute_workflow))
            .function(wrap_function!(engine_ai_register_tool_callback))
            .function(wrap_function!(engine_ai_clear_tool_callback))
            .function(wrap_function!(engine_ai_module_info))
    }
}
