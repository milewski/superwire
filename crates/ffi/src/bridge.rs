use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use engine_ai_agent::DynamicTool;
use engine_ai_agent::ToolError;
use engine_ai_core::dsl::{Declaration, TypeExpression, Workflow};
use engine_ai_core::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use engine_ai_core::runtime::types::{workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType};
use engine_ai_core::runtime::WorkflowRuntimeError;
use engine_ai_core::runtime::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, LoopAgentRunner, WorkflowRuntime};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::FfiError;
use crate::types::{
    CustomToolDeclaration, FfiRequest, FfiRequestEnvelope, FfiResponse, FfiResponseEnvelope, ToolCallbackConfig, ToolInvocationEnvelope,
    ToolInvocationError, ToolInvocationErrorCode, ToolInvocationPayload, ToolInvocationResult, WorkflowExecutionEnvelope,
    WorkflowExecutionError, WorkflowExecutionErrorCode, WorkflowExecutionOutput, WorkflowExecutionRequest, FFI_PROTOCOL_VERSION,
};

static TOOL_INVOCATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static DYNAMIC_WORKFLOW_SCHEMA_CONTEXT: RefCell<Option<FfiRuntimeSchemaContext>> = const { RefCell::new(None) };
}

type CustomToolHandlerFuture = Pin<Box<dyn Future<Output = Result<Value, ToolInvocationError>> + Send>>;

pub type CustomToolHandler = Arc<dyn Fn(ToolInvocationPayload) -> CustomToolHandlerFuture + Send + Sync>;

#[derive(Default)]
pub struct EngineFfi {
    custom_tool_registry: Arc<CustomToolRegistry>,
}

impl EngineFfi {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_custom_tool_handler<Handler, HandlerFuture>(&self, tool_name: impl Into<String>, handler: Handler)
    where
        Handler: Fn(ToolInvocationPayload) -> HandlerFuture + Send + Sync + 'static,
        HandlerFuture: Future<Output = Result<Value, ToolInvocationError>> + Send + 'static,
    {
        let wrapped_handler =
            Arc::new(move |invocation_payload: ToolInvocationPayload| Box::pin(handler(invocation_payload)) as CustomToolHandlerFuture);

        self.custom_tool_registry.register_handler(tool_name.into(), wrapped_handler);
    }

    pub fn register_custom_tool_handler_callback(&self, tool_name: impl Into<String>, handler: CustomToolHandler) {
        self.custom_tool_registry.register_handler(tool_name.into(), handler);
    }

    pub fn invoke(&self, request_envelope: FfiRequestEnvelope) -> Result<FfiResponseEnvelope, FfiError> {
        let request_id = request_envelope.request_id;
        let response = match request_envelope.request {
            FfiRequest::ExecuteWorkflow(workflow_execution_request) => {
                FfiResponse::ExecuteWorkflow(self.execute_workflow(workflow_execution_request))
            }
            FfiRequest::InvokeTool(tool_invocation_payload) => FfiResponse::InvokeTool(self.invoke_tool(tool_invocation_payload)),
        };

        Ok(FfiResponseEnvelope {
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id,
            response,
        })
    }

    fn execute_workflow(&self, workflow_execution_request: WorkflowExecutionRequest) -> WorkflowExecutionEnvelope {
        if let Err(workflow_execution_error) = workflow_execution_request.register_custom_tools(&self.custom_tool_registry) {
            return WorkflowExecutionEnvelope::Failed {
                error: workflow_execution_error,
            };
        }

        let parsed_workflow = match workflow_execution_request.parse_workflow() {
            Ok(parsed_workflow) => parsed_workflow,
            Err(workflow_execution_error) => {
                return WorkflowExecutionEnvelope::Failed {
                    error: workflow_execution_error,
                };
            }
        };

        let runtime_schema_context = match FfiRuntimeSchemaContext::from_workflow(&parsed_workflow) {
            Ok(runtime_schema_context) => runtime_schema_context,
            Err(workflow_execution_error) => {
                return WorkflowExecutionEnvelope::Failed {
                    error: workflow_execution_error,
                };
            }
        };

        let runtime = match Self::build_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                return WorkflowExecutionEnvelope::Failed {
                    error: WorkflowExecutionError::internal(error.to_string()),
                };
            }
        };

        let execution_result = runtime_schema_context.with_scope(|| {
            runtime.block_on(async {
                let workflow_runtime =
                    WorkflowRuntime::<DynamicWorkflowInputValue, DynamicWorkflowOutputValue>::new(parsed_workflow.clone())?;

                let runtime_tools = self
                    .custom_tool_registry
                    .runtime_tools_for_execution(&workflow_execution_request.execution_id, &workflow_execution_request.custom_tools)?;

                let ffi_agent_runner = FfiAgentRunner::new(runtime_tools);

                let workflow_output = workflow_runtime
                    .run_with_runner(
                        DynamicWorkflowInputValue::from(workflow_execution_request.input.payload.clone()),
                        &ffi_agent_runner,
                    )
                    .await?;

                Ok::<DynamicWorkflowOutputValue, WorkflowRuntimeError>(workflow_output)
            })
        });

        match execution_result {
            Ok(workflow_output) => WorkflowExecutionEnvelope::Succeeded {
                output: WorkflowExecutionOutput {
                    execution_id: workflow_execution_request.execution_id,
                    output: workflow_output.into_inner(),
                },
            },
            Err(runtime_error) => WorkflowExecutionEnvelope::Failed {
                error: WorkflowExecutionError::from_runtime_error(runtime_error),
            },
        }
    }

    fn invoke_tool(&self, tool_invocation_payload: ToolInvocationPayload) -> ToolInvocationEnvelope {
        let runtime = match Self::build_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                return ToolInvocationEnvelope::Failed {
                    error: ToolInvocationError::internal(error.to_string()),
                };
            }
        };

        let invocation_result = runtime.block_on(self.custom_tool_registry.invoke(tool_invocation_payload.clone()));

        match invocation_result {
            Ok(tool_output) => ToolInvocationEnvelope::Succeeded {
                result: ToolInvocationResult {
                    execution_id: tool_invocation_payload.execution_id,
                    invocation_id: tool_invocation_payload.invocation_id,
                    output: tool_output,
                },
            },
            Err(tool_invocation_error) => ToolInvocationEnvelope::Failed {
                error: tool_invocation_error,
            },
        }
    }

    fn build_runtime() -> Result<tokio::runtime::Runtime, FfiError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| FfiError::RuntimeInitializationFailed {
                message: error.to_string(),
            })
    }
}

#[derive(Debug, Clone)]
struct FfiAgentRunner {
    loop_runner: LoopAgentRunner,
    runtime_tools: Vec<DynamicTool>,
}

impl FfiAgentRunner {
    fn new(runtime_tools: Vec<DynamicTool>) -> Self {
        Self {
            loop_runner: LoopAgentRunner,
            runtime_tools,
        }
    }
}

#[async_trait::async_trait]
impl AgentRunner for FfiAgentRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        let mut runtime_tools = request.runtime_tools.clone();
        runtime_tools.extend(self.runtime_tools.clone());

        let bridged_request = AgentExecutionRequest {
            agent_name: request.agent_name.clone(),
            provider_config: request.provider_config.clone(),
            model_name: request.model_name.clone(),
            prompt: request.prompt.clone(),
            config: request.config.clone(),
            output_schema: request.output_schema.clone(),
            requested_tools: request.requested_tools.clone(),
            runtime_tools,
        };

        self.loop_runner.run_agent(&bridged_request).await
    }
}

#[derive(Default)]
struct CustomToolRegistry {
    handlers_by_name: RwLock<HashMap<String, CustomToolHandler>>,
    declarations_by_execution: RwLock<HashMap<String, HashMap<String, CustomToolDeclaration>>>,
}

impl CustomToolRegistry {
    fn register_handler(&self, tool_name: String, custom_tool_handler: CustomToolHandler) {
        let mut handlers_by_name = self
            .handlers_by_name
            .write()
            .expect("custom tool handler registry lock should not be poisoned");

        handlers_by_name.insert(tool_name, custom_tool_handler);
    }

    fn has_handler(&self, tool_name: &str) -> bool {
        let handlers_by_name = self
            .handlers_by_name
            .read()
            .expect("custom tool handler registry lock should not be poisoned");

        handlers_by_name.contains_key(tool_name)
    }

    fn ensure_callback_handlers(
        &self,
        custom_tools: &[CustomToolDeclaration],
        tool_callback: Option<&ToolCallbackConfig>,
    ) -> Result<(), WorkflowExecutionError> {
        let Some(tool_callback) = tool_callback else {
            return Ok(());
        };

        for custom_tool in custom_tools {
            if self.has_handler(&custom_tool.name) {
                continue;
            }

            let callback_tool_name = custom_tool.name.clone();
            let callback_config = tool_callback.clone();

            self.register_handler(
                custom_tool.name.clone(),
                Arc::new(move |tool_invocation_payload: ToolInvocationPayload| {
                    let callback_tool_name = callback_tool_name.clone();
                    let callback_config = callback_config.clone();

                    Box::pin(async move {
                        callback_config.invoke_tool(&tool_invocation_payload).await.map_err(|error| {
                            if error.code == ToolInvocationErrorCode::ToolNotFound {
                                return error;
                            }

                            ToolInvocationError {
                                code: ToolInvocationErrorCode::ExecutionFailed,
                                message: format!(
                                    "failed to invoke callback handler for custom tool `{}`: {}",
                                    callback_tool_name, error.message
                                ),
                                details: error.details,
                            }
                        })
                    })
                }),
            );
        }

        Ok(())
    }

    fn register_execution_tools(&self, execution_id: &str, custom_tools: &[CustomToolDeclaration]) -> Result<(), WorkflowExecutionError> {
        let mut declarations_by_name = HashMap::new();

        for custom_tool in custom_tools {
            if declarations_by_name.insert(custom_tool.name.clone(), custom_tool.clone()).is_some() {
                return Err(WorkflowExecutionError::tool_invocation_failed(
                    format!("duplicate custom tool declaration `{}`", custom_tool.name),
                    None,
                ));
            }
        }

        let mut declarations_by_execution = self
            .declarations_by_execution
            .write()
            .expect("custom tool declaration registry lock should not be poisoned");

        declarations_by_execution.insert(execution_id.to_string(), declarations_by_name);

        Ok(())
    }

    async fn invoke(&self, tool_invocation_payload: ToolInvocationPayload) -> Result<Value, ToolInvocationError> {
        let custom_tool_handler = self.resolve_handler(&tool_invocation_payload)?;

        custom_tool_handler(tool_invocation_payload).await
    }

    fn runtime_tools_for_execution(
        self: &Arc<Self>,
        execution_id: &str,
        custom_tools: &[CustomToolDeclaration],
    ) -> Result<Vec<DynamicTool>, WorkflowRuntimeError> {
        let declarations_by_execution = self
            .declarations_by_execution
            .read()
            .expect("custom tool declaration registry lock should not be poisoned");

        let Some(declarations_by_name) = declarations_by_execution.get(execution_id) else {
            return Err(WorkflowRuntimeError::Other {
                message: format!("unknown execution `{execution_id}` when preparing custom runtime tools"),
            });
        };

        let mut runtime_tools = Vec::new();

        for custom_tool in custom_tools {
            let Some(declared_tool) = declarations_by_name.get(&custom_tool.name) else {
                continue;
            };

            let parameters_schema =
                serde_json::from_value::<Schema>(declared_tool.input_schema.clone()).map_err(|error| WorkflowRuntimeError::Other {
                    message: format!(
                        "failed to parse input schema for custom tool `{}` in execution `{execution_id}`: {error}",
                        declared_tool.name
                    ),
                })?;

            let registry = Arc::clone(self);
            let execution_id = execution_id.to_string();
            let tool_name = declared_tool.name.clone();

            let runtime_tool = DynamicTool::from_parts(
                declared_tool.name.clone(),
                declared_tool
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Custom tool `{}` registered through FFI", declared_tool.name)),
                parameters_schema,
                move |arguments| {
                    let registry = Arc::clone(&registry);
                    let execution_id = execution_id.clone();
                    let tool_name = tool_name.clone();

                    async move {
                        let tool_invocation_payload = ToolInvocationPayload::from_runtime_request(execution_id, tool_name, arguments);

                        registry.invoke(tool_invocation_payload).await.map_err(|error| {
                            let mut tool_error = ToolError::new(error.message);

                            if let Some(error_details) = error.details {
                                tool_error = tool_error.with_context("details", error_details);
                            }

                            tool_error
                        })
                    }
                },
            );

            runtime_tools.push(runtime_tool);
        }

        Ok(runtime_tools)
    }

    fn resolve_handler(&self, tool_invocation_payload: &ToolInvocationPayload) -> Result<CustomToolHandler, ToolInvocationError> {
        let declarations_by_execution = self
            .declarations_by_execution
            .read()
            .expect("custom tool declaration registry lock should not be poisoned");

        let Some(declarations_by_name) = declarations_by_execution.get(&tool_invocation_payload.execution_id) else {
            return Err(ToolInvocationError::tool_not_found(format!(
                "unknown execution `{}`",
                tool_invocation_payload.execution_id
            )));
        };

        if !declarations_by_name.contains_key(&tool_invocation_payload.tool_name) {
            return Err(ToolInvocationError::tool_not_found(format!(
                "tool `{}` was not declared for execution `{}`",
                tool_invocation_payload.tool_name, tool_invocation_payload.execution_id
            )));
        }

        let handlers_by_name = self
            .handlers_by_name
            .read()
            .expect("custom tool handler registry lock should not be poisoned");

        let Some(custom_tool_handler) = handlers_by_name.get(&tool_invocation_payload.tool_name) else {
            return Err(ToolInvocationError::tool_not_found(format!(
                "no handler registered for custom tool `{}`",
                tool_invocation_payload.tool_name
            )));
        };

        Ok(Arc::clone(custom_tool_handler))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct DynamicWorkflowInputValue {
    value: Value,
}

impl From<Value> for DynamicWorkflowInputValue {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct DynamicWorkflowOutputValue {
    value: Value,
}

impl DynamicWorkflowOutputValue {
    fn into_inner(self) -> Value {
        self.value
    }
}

impl From<Value> for DynamicWorkflowOutputValue {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

impl JsonSchema for DynamicWorkflowInputValue {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowInputValue")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let runtime_schema_context = runtime_schema_context_cell.borrow();
            let runtime_schema_context = runtime_schema_context
                .as_ref()
                .expect("dynamic workflow schema context must be initialized before execution");

            runtime_schema_context.input_schema()
        })
    }
}

impl JsonSchema for DynamicWorkflowOutputValue {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowOutputValue")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let runtime_schema_context = runtime_schema_context_cell.borrow();
            let runtime_schema_context = runtime_schema_context
                .as_ref()
                .expect("dynamic workflow schema context must be initialized before execution");

            runtime_schema_context.output_schema()
        })
    }
}

#[derive(Debug, Clone)]
struct FfiRuntimeSchemaContext {
    input_schema: Schema,
    output_schema: Schema,
}

impl FfiRuntimeSchemaContext {
    fn from_workflow(workflow: &Workflow) -> Result<Self, WorkflowExecutionError> {
        let workflow_type_inference = WorkflowTypeInference::from_workflow(workflow)?;

        let inferred_input_type = workflow_type_inference
            .input_type
            .unwrap_or_else(|| WorkflowType::Object(BTreeMap::new()));
        let inferred_output_type = workflow_type_inference.workflow_output_type;
        let input_schema_value = workflow_type_to_json_schema(&inferred_input_type);
        let output_schema_value = workflow_type_to_json_schema(&inferred_output_type);
        let input_schema = serde_json::from_value::<Schema>(input_schema_value).map_err(|error| {
            WorkflowExecutionError::validation_failed(format!("failed to convert inferred workflow input type into schema: {error}"), None)
        })?;

        let output_schema = serde_json::from_value::<Schema>(output_schema_value).map_err(|error| {
            WorkflowExecutionError::validation_failed(
                format!("failed to convert inferred workflow output type into schema: {error}"),
                None,
            )
        })?;

        Ok(Self {
            input_schema,
            output_schema,
        })
    }

    fn input_schema(&self) -> Schema {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Schema {
        self.output_schema.clone()
    }

    fn with_scope<ExecutionResult>(&self, execute_with_schema_context: impl FnOnce() -> ExecutionResult) -> ExecutionResult {
        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let previous_context = runtime_schema_context_cell.replace(Some(self.clone()));
            let execution_result = execute_with_schema_context();
            runtime_schema_context_cell.replace(previous_context);

            execution_result
        })
    }
}

#[derive(Debug, Clone)]
struct WorkflowTypeInference {
    input_type: Option<WorkflowType>,
    workflow_output_type: WorkflowType,
}

impl WorkflowTypeInference {
    fn from_workflow(workflow: &Workflow) -> Result<Self, WorkflowExecutionError> {
        let named_schema_types = Self::collect_named_schema_types(workflow);
        let input_type = Self::build_input_type(workflow, &named_schema_types)?;
        let secrets_type = Self::build_secrets_type(workflow, &named_schema_types)?;
        let agent_output_types = Self::collect_agent_output_types(workflow, &named_schema_types)?;
        let workflow_output_type = Self::infer_workflow_output_type(workflow, input_type.clone(), secrets_type, agent_output_types)?;

        Ok(Self {
            input_type,
            workflow_output_type,
        })
    }

    fn collect_named_schema_types(workflow: &Workflow) -> HashMap<String, TypeExpression> {
        let mut named_schema_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Schema(schema_declaration) = declaration else {
                continue;
            };

            named_schema_types.insert(
                schema_declaration.name.clone(),
                TypeExpression::Object(schema_declaration.fields.clone()),
            );
        }

        named_schema_types
    }

    fn build_input_type(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Option<WorkflowType>, WorkflowExecutionError> {
        let Some(input_declaration) = workflow.find_input() else {
            return Ok(None);
        };

        let input_type_expression = TypeExpression::Object(input_declaration.fields.clone());
        let input_type = workflow_type_from_dsl(&input_type_expression, named_schema_types)
            .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

        Ok(Some(input_type))
    }

    fn build_secrets_type(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Option<WorkflowType>, WorkflowExecutionError> {
        let Some(secrets_declaration) = workflow.find_secrets() else {
            return Ok(None);
        };

        let secrets_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
        let secrets_type = workflow_type_from_dsl(&secrets_type_expression, named_schema_types)
            .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

        Ok(Some(secrets_type))
    }

    fn collect_agent_output_types(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<HashMap<String, WorkflowType>, WorkflowExecutionError> {
        let mut agent_output_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let iteration_output_type = if let Some(agent_output_type_expression) = agent_declaration.output_type() {
                workflow_type_from_dsl(agent_output_type_expression, named_schema_types)
                    .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?
            } else {
                WorkflowType::String
            };

            let final_output_type = if agent_declaration.for_loop.is_some() {
                WorkflowType::Array {
                    item_type: Box::new(iteration_output_type),
                    fixed_length: None,
                }
                .normalize()
            } else {
                iteration_output_type
            };

            agent_output_types.insert(agent_declaration.name.clone(), final_output_type);
        }

        Ok(agent_output_types)
    }

    fn infer_workflow_output_type(
        workflow: &Workflow,
        input_type: Option<WorkflowType>,
        secrets_type: Option<WorkflowType>,
        agent_output_types: HashMap<String, WorkflowType>,
    ) -> Result<WorkflowType, WorkflowExecutionError> {
        let Some(output_declaration) = workflow.find_output() else {
            return Err(WorkflowExecutionError::validation_failed(
                String::from("workflow requires an `output` block"),
                None,
            ));
        };

        let type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            local_binding_types: HashMap::new(),
        };

        let mut output_field_types = BTreeMap::new();

        for output_field in &output_declaration.fields {
            let output_field_type =
                infer_expression_type(&output_field.value, &type_inference_context, "ffi workflow output type inference")
                    .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

            output_field_types.insert(output_field.name.clone(), output_field_type);
        }

        Ok(WorkflowType::Object(output_field_types).normalize())
    }
}

impl WorkflowExecutionRequest {
    fn register_custom_tools(&self, custom_tool_registry: &Arc<CustomToolRegistry>) -> Result<(), WorkflowExecutionError> {
        custom_tool_registry.ensure_callback_handlers(&self.custom_tools, self.tool_callback.as_ref())?;

        let missing_tool_handlers = self
            .custom_tools
            .iter()
            .filter(|custom_tool| !custom_tool_registry.has_handler(&custom_tool.name))
            .map(|custom_tool| custom_tool.name.clone())
            .collect::<Vec<_>>();

        if !missing_tool_handlers.is_empty() {
            return Err(WorkflowExecutionError::tool_invocation_failed(
                format!("missing custom tool handlers for: {}", missing_tool_handlers.join(", ")),
                Some(serde_json::json!({
                    "execution_id": self.execution_id,
                    "missing_tools": missing_tool_handlers,
                })),
            ));
        }

        custom_tool_registry.register_execution_tools(&self.execution_id, &self.custom_tools)
    }

    fn parse_workflow(&self) -> Result<Workflow, WorkflowExecutionError> {
        engine_ai_core::dsl::parse_workflow(&self.workflow_source).map_err(|parse_error| {
            let rendered_error_details = parse_error.render_with_source(&self.workflow_source, "ffi://workflow");

            WorkflowExecutionError::parse_failed(
                parse_error.to_string(),
                Some(serde_json::json!({
                    "rendered": rendered_error_details,
                })),
            )
        })
    }
}

impl ToolInvocationPayload {
    fn from_runtime_request(execution_id: String, tool_name: String, arguments: Value) -> Self {
        let invocation_sequence = TOOL_INVOCATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);

        Self {
            execution_id,
            invocation_id: format!("invocation-{invocation_sequence}"),
            tool_name,
            arguments,
        }
    }
}

impl ToolCallbackConfig {
    async fn invoke_tool(&self, tool_invocation_payload: &ToolInvocationPayload) -> Result<Value, ToolInvocationError> {
        let mut http_request_builder = reqwest::Client::new().post(&self.endpoint).json(tool_invocation_payload);

        if let Some(auth_token) = &self.auth_token {
            http_request_builder = http_request_builder.header("x-engine-ai-tool-callback-token", auth_token);
        }

        let http_response = http_request_builder.send().await.map_err(|error| ToolInvocationError {
            code: ToolInvocationErrorCode::ExecutionFailed,
            message: format!("callback request failed: {error}"),
            details: None,
        })?;

        let callback_response_envelope = http_response
            .json::<ToolInvocationEnvelope>()
            .await
            .map_err(|error| ToolInvocationError {
                code: ToolInvocationErrorCode::ExecutionFailed,
                message: format!("callback response parsing failed: {error}"),
                details: None,
            })?;

        match callback_response_envelope {
            ToolInvocationEnvelope::Succeeded { result } => Ok(result.output),
            ToolInvocationEnvelope::Failed { error } => Err(error),
        }
    }
}

impl WorkflowExecutionError {
    fn parse_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ParseFailed,
            message,
            details,
        }
    }

    fn validation_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ValidationFailed,
            message,
            details,
        }
    }

    fn runtime_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::RuntimeFailed,
            message,
            details,
        }
    }

    fn tool_invocation_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ToolInvocationFailed,
            message,
            details,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::Internal,
            message,
            details: None,
        }
    }

    fn from_runtime_error(runtime_error: WorkflowRuntimeError) -> Self {
        match runtime_error {
            WorkflowRuntimeError::ParseFailed { source: _, details } => Self::parse_failed(details, None),
            WorkflowRuntimeError::InvalidWorkflow { issues }
            | WorkflowRuntimeError::ExecutionPlanInvariant { message: issues }
            | WorkflowRuntimeError::MissingDeclaration { message: issues }
            | WorkflowRuntimeError::UnsupportedFeature { feature: issues }
            | WorkflowRuntimeError::InputTypeMismatch {
                expected: issues,
                found: _,
            }
            | WorkflowRuntimeError::OutputTypeMismatch {
                expected: issues,
                found: _,
            } => Self::validation_failed(issues, None),
            WorkflowRuntimeError::ProviderConfiguration { provider_name, message } => {
                Self::runtime_failed(format!("provider `{provider_name}` configuration error: {message}"), None)
            }
            WorkflowRuntimeError::ExpressionEvaluation { context, message } => {
                Self::runtime_failed(format!("expression evaluation failed in {context}: {message}"), None)
            }
            WorkflowRuntimeError::InvalidAgentProperty {
                agent_name,
                property,
                message,
            } => Self::runtime_failed(format!("agent `{agent_name}` has invalid `{property}` property: {message}"), None),
            WorkflowRuntimeError::InputValueMismatch { message } => Self::runtime_failed(message, None),
            WorkflowRuntimeError::AgentOutputTypeMismatch { agent_name, message } => {
                Self::runtime_failed(format!("agent `{agent_name}` output does not match declared type: {message}"), None)
            }
            WorkflowRuntimeError::AgentExecutionFailed { agent_name, source } => {
                Self::runtime_failed(format!("agent `{agent_name}` execution failed: {source}"), None)
            }
            WorkflowRuntimeError::SerializationFailed { context, source } => {
                Self::runtime_failed(format!("serialization failed for {context}: {source}"), None)
            }
            WorkflowRuntimeError::OutputDeserializationFailed { source } => {
                Self::runtime_failed(format!("output deserialization failed: {source}"), None)
            }
            WorkflowRuntimeError::Other { message } => Self::runtime_failed(message, None),
        }
    }
}

impl ToolInvocationError {
    fn tool_not_found(message: String) -> Self {
        Self {
            code: ToolInvocationErrorCode::ToolNotFound,
            message,
            details: None,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            code: ToolInvocationErrorCode::Internal,
            message,
            details: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::EngineFfi;
    use crate::types::{
        CustomToolDeclaration, FfiRequest, FfiRequestEnvelope, ToolInvocationEnvelope, ToolInvocationErrorCode, ToolInvocationPayload,
        WorkflowExecutionEnvelope, WorkflowExecutionInput, WorkflowExecutionRequest,
    };

    #[test]
    fn executes_workflow_request_and_returns_json_output() {
        let workflow_execution_request = WorkflowExecutionRequest {
            execution_id: String::from("execution-1"),
            workflow_source: String::from(
                r"
                input {
                    name: string
                }

                output {
                    greeting: input.name
                }
                ",
            ),
            input: WorkflowExecutionInput {
                payload: json!({ "name": "hello from ffi" }),
            },
            custom_tools: Vec::new(),
            tool_callback: None,
        };

        let request_envelope = FfiRequestEnvelope::new(FfiRequest::ExecuteWorkflow(workflow_execution_request));
        let engine_ffi = EngineFfi::new();
        let response_envelope = engine_ffi.invoke(request_envelope).expect("ffi invocation should not fail");

        let WorkflowExecutionEnvelope::Succeeded { output } = response_envelope.response.expect_execute_workflow() else {
            panic!("workflow execution should succeed");
        };

        assert_eq!(output.output, json!({ "greeting": "hello from ffi" }));
    }

    #[test]
    fn invokes_registered_custom_tool_handler() {
        let engine_ffi = EngineFfi::new();
        engine_ffi.register_custom_tool_handler("echo", |tool_invocation_payload| async move {
            Ok(json!({
                "received": tool_invocation_payload.arguments,
            }))
        });

        let workflow_execution_request = WorkflowExecutionRequest {
            execution_id: String::from("execution-with-tool"),
            workflow_source: String::from(
                r"
                output {
                    ok: true
                }
                ",
            ),
            input: WorkflowExecutionInput { payload: json!({}) },
            custom_tools: vec![CustomToolDeclaration {
                name: String::from("echo"),
                description: Some(String::from("Echo tool")),
                input_schema: json!({ "type": "object" }),
                output_schema: None,
            }],
            tool_callback: None,
        };

        let workflow_request_envelope = FfiRequestEnvelope::new(FfiRequest::ExecuteWorkflow(workflow_execution_request));
        let _workflow_response = engine_ffi
            .invoke(workflow_request_envelope)
            .expect("workflow invocation should not fail");

        let tool_request_envelope = FfiRequestEnvelope::new(FfiRequest::InvokeTool(ToolInvocationPayload {
            execution_id: String::from("execution-with-tool"),
            invocation_id: String::from("invocation-1"),
            tool_name: String::from("echo"),
            arguments: json!({ "message": "hello tool" }),
        }));

        let response_envelope = engine_ffi.invoke(tool_request_envelope).expect("tool invocation should not fail");

        let ToolInvocationEnvelope::Succeeded { result } = response_envelope.response.expect_invoke_tool() else {
            panic!("tool invocation should succeed");
        };

        assert_eq!(result.output, json!({ "received": { "message": "hello tool" } }));
    }

    #[test]
    fn returns_tool_not_found_when_execution_is_unknown() {
        let engine_ffi = EngineFfi::new();
        let request_envelope = FfiRequestEnvelope::new(FfiRequest::InvokeTool(ToolInvocationPayload {
            execution_id: String::from("missing-execution"),
            invocation_id: String::from("invocation-1"),
            tool_name: String::from("unknown"),
            arguments: json!({}),
        }));

        let response_envelope = engine_ffi
            .invoke(request_envelope)
            .expect("ffi invocation should not fail on typed envelope errors");

        let ToolInvocationEnvelope::Failed { error } = response_envelope.response.expect_invoke_tool() else {
            panic!("tool invocation should fail for unknown execution");
        };

        assert_eq!(error.code, ToolInvocationErrorCode::ToolNotFound);
    }

    trait ResponseEnvelopeExpectExt {
        fn expect_execute_workflow(self) -> WorkflowExecutionEnvelope;

        fn expect_invoke_tool(self) -> ToolInvocationEnvelope;
    }

    impl ResponseEnvelopeExpectExt for crate::types::FfiResponse {
        fn expect_execute_workflow(self) -> WorkflowExecutionEnvelope {
            let crate::types::FfiResponse::ExecuteWorkflow(workflow_execution_envelope) = self else {
                panic!("expected execute_workflow response");
            };

            workflow_execution_envelope
        }

        fn expect_invoke_tool(self) -> ToolInvocationEnvelope {
            let crate::types::FfiResponse::InvokeTool(tool_invocation_envelope) = self else {
                panic!("expected invoke_tool response");
            };

            tool_invocation_envelope
        }
    }
}
