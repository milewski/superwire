use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;
use superwire_core::dsl::Workflow;
use superwire_core::runtime::WorkflowRuntimeError;
use superwire_core::runtime::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, LoopAgentRunner, WorkflowRuntime};

use crate::error::FfiError;
use crate::types::{
    FfiRequest, FfiRequestEnvelope, FfiResponse, FfiResponseEnvelope, ReadExecutionValueEnvelope, ReadExecutionValueRequest,
    ReadExecutionValueSuccess, ToolCallbackConfig, ToolInvocationEnvelope, ToolInvocationError, ToolInvocationErrorCode,
    ToolInvocationPayload, ToolInvocationResult, WorkflowExecutionEnvelope, WorkflowExecutionError, WorkflowExecutionOutput,
    WorkflowExecutionRequest, FFI_PROTOCOL_VERSION,
};

mod custom_tool_registry;
mod error_mapping;
mod execution_store;
mod runtime_schema;

use custom_tool_registry::CustomToolRegistry;
use execution_store::ExecutionResultStore;
use runtime_schema::{DynamicWorkflowInputValue, DynamicWorkflowOutputValue, FfiRuntimeSchemaContext};

static TOOL_INVOCATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static EXECUTION_RESULT_STORE: OnceLock<Arc<ExecutionResultStore>> = OnceLock::new();
static SHARED_ASYNC_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

const MAX_REGISTERED_EXECUTIONS: usize = 256;

type CustomToolHandlerFuture = Pin<Box<dyn Future<Output = Result<Value, ToolInvocationError>> + Send>>;

pub type CustomToolHandler = Arc<dyn Fn(ToolInvocationPayload) -> CustomToolHandlerFuture + Send + Sync>;

pub struct EngineFfi {
    custom_tool_registry: Arc<CustomToolRegistry>,
    execution_result_store: Arc<ExecutionResultStore>,
}

impl Default for EngineFfi {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineFfi {
    #[must_use]
    pub fn new() -> Self {
        Self {
            custom_tool_registry: Arc::new(CustomToolRegistry::default()),
            execution_result_store: Arc::clone(shared_execution_result_store()),
        }
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
            FfiRequest::ReadExecutionValue(read_execution_value_request) => {
                FfiResponse::ReadExecutionValue(self.read_execution_value(read_execution_value_request))
            }
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

        let runtime = match Self::shared_runtime() {
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

                let ffi_agent_runner = FfiAgentRunner::new(
                    Arc::clone(&self.custom_tool_registry),
                    workflow_execution_request.execution_id.clone(),
                    workflow_execution_request.input.payload.clone(),
                );

                let workflow_output = workflow_runtime
                    .run_with_runner_and_secrets(
                        DynamicWorkflowInputValue::from(workflow_execution_request.input.payload.clone()),
                        workflow_execution_request.secrets_payload(),
                        &ffi_agent_runner,
                    )
                    .await?;

                Ok::<(DynamicWorkflowOutputValue, Value), WorkflowRuntimeError>((workflow_output, ffi_agent_runner.snapshot_context()))
            })
        });

        match execution_result {
            Ok((workflow_output, agent_context)) => WorkflowExecutionEnvelope::Succeeded {
                output: WorkflowExecutionOutput {
                    execution_id: workflow_execution_request.execution_id.clone(),
                    output: if workflow_execution_request.defer_output {
                        self.execution_result_store.insert_success(
                            workflow_execution_request.execution_id,
                            workflow_output.into_inner(),
                            agent_context,
                        );

                        None
                    } else {
                        Some(workflow_output.into_inner())
                    },
                },
            },
            Err(runtime_error) => {
                let workflow_execution_error = WorkflowExecutionError::from_runtime_error(runtime_error);

                if workflow_execution_request.defer_output {
                    self.execution_result_store.insert_failure(
                        workflow_execution_request.execution_id,
                        workflow_execution_error.clone(),
                        Value::Null,
                    );
                }

                WorkflowExecutionEnvelope::Failed {
                    error: workflow_execution_error,
                }
            }
        }
    }

    fn invoke_tool(&self, tool_invocation_payload: ToolInvocationPayload) -> ToolInvocationEnvelope {
        let runtime = match Self::shared_runtime() {
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

    fn read_execution_value(&self, read_execution_value_request: ReadExecutionValueRequest) -> ReadExecutionValueEnvelope {
        match self
            .execution_result_store
            .read_value(&read_execution_value_request.execution_id, read_execution_value_request.value)
        {
            Ok(value) => ReadExecutionValueEnvelope::Succeeded {
                result: ReadExecutionValueSuccess {
                    execution_id: read_execution_value_request.execution_id,
                    value,
                },
            },
            Err(error) => ReadExecutionValueEnvelope::Failed { error },
        }
    }

    fn shared_runtime() -> Result<&'static tokio::runtime::Runtime, FfiError> {
        let shared_runtime_result = SHARED_ASYNC_RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
        });

        match shared_runtime_result {
            Ok(runtime) => Ok(runtime),
            Err(error_message) => Err(FfiError::RuntimeInitializationFailed {
                message: error_message.clone(),
            }),
        }
    }
}

fn shared_execution_result_store() -> &'static Arc<ExecutionResultStore> {
    EXECUTION_RESULT_STORE.get_or_init(|| Arc::new(ExecutionResultStore::default()))
}

#[derive(Clone)]
struct FfiAgentRunner {
    loop_runner: LoopAgentRunner,
    custom_tool_registry: Arc<CustomToolRegistry>,
    execution_id: String,
    workflow_input: Value,
    captured_agent_context: Arc<RwLock<HashMap<String, Value>>>,
}

impl FfiAgentRunner {
    fn new(custom_tool_registry: Arc<CustomToolRegistry>, execution_id: String, workflow_input: Value) -> Self {
        Self {
            loop_runner: LoopAgentRunner,
            custom_tool_registry,
            execution_id,
            workflow_input,
            captured_agent_context: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn snapshot_context(&self) -> Value {
        let agent_context = self
            .captured_agent_context
            .read()
            .expect("captured agent context lock should not be poisoned");

        let mut context_value = serde_json::Map::new();

        for (agent_name, agent_context_value) in &*agent_context {
            context_value.insert(agent_name.clone(), agent_context_value.clone());
        }

        Value::Object(context_value)
    }
}

#[async_trait::async_trait]
impl AgentRunner for FfiAgentRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        let runtime_tools = self.custom_tool_registry.runtime_tools_for_requested_tools(
            &self.execution_id,
            &request.requested_tools,
            &self.workflow_input,
        )?;

        let requested_tools_without_bound_arguments = request
            .requested_tools
            .iter()
            .map(|requested_tool| superwire_core::runtime::RequestedAgentTool {
                name: requested_tool.name.clone(),
                bound_arguments: serde_json::Map::new(),
            })
            .collect::<Vec<_>>();

        let bridged_request = AgentExecutionRequest {
            agent_name: request.agent_name.clone(),
            provider_config: request.provider_config.clone(),
            model_name: request.model_name.clone(),
            prompt: request.prompt.clone(),
            context: request.context.clone(),
            config: request.config.clone(),
            output_schema: request.output_schema.clone(),
            requested_tools: requested_tools_without_bound_arguments,
            runtime_tools,
        };

        let agent_result = self.loop_runner.run_agent(&bridged_request).await?;

        self.captured_agent_context
            .write()
            .expect("captured agent context lock should not be poisoned")
            .insert(request.agent_name.clone(), agent_result.context.clone());

        Ok(agent_result)
    }
}

impl WorkflowExecutionRequest {
    fn secrets_payload(&self) -> Value {
        self.secrets
            .as_ref()
            .map_or(Value::Null, |workflow_secrets| workflow_secrets.payload.clone())
    }

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
        superwire_core::dsl::parse_workflow(&self.workflow_source).map_err(|parse_error| {
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
    fn from_runtime_request(execution_id: String, tool_name: String, arguments: Value, execution_context: Option<Value>) -> Self {
        let invocation_sequence = TOOL_INVOCATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);

        Self {
            execution_id,
            invocation_id: format!("invocation-{invocation_sequence}"),
            tool_name,
            arguments,
            execution_context,
        }
    }
}

impl ToolCallbackConfig {
    async fn invoke_tool(&self, tool_invocation_payload: &ToolInvocationPayload) -> Result<Value, ToolInvocationError> {
        let mut http_request_builder = reqwest::Client::new().post(&self.endpoint).json(tool_invocation_payload);

        if let Some(auth_token) = &self.auth_token {
            http_request_builder = http_request_builder.header("x-superwire-tool-callback-token", auth_token);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use superwire_agent::{AgentError, ToolError};
    use superwire_core::runtime::WorkflowRuntimeError;

    use super::{CustomToolRegistry, EngineFfi, MAX_REGISTERED_EXECUTIONS};
    use crate::types::WorkflowExecutionError;
    use crate::types::{
        CustomToolDeclaration, FfiRequest, FfiRequestEnvelope, ToolInvocationEnvelope, ToolInvocationErrorCode, ToolInvocationPayload,
        WorkflowExecutionEnvelope, WorkflowExecutionErrorCode, WorkflowExecutionInput, WorkflowExecutionRequest,
    };

    #[test]
    fn separates_agent_context_from_runtime_error_message() {
        let agent_error = AgentError::from(ToolError::new("Unable to connect to the weather service"));

        let runtime_error = WorkflowRuntimeError::AgentExecutionFailed {
            agent_name: String::from("assistant"),
            source: Box::new(agent_error),
        };

        let workflow_execution_error = WorkflowExecutionError::from_runtime_error(runtime_error);

        assert_eq!(workflow_execution_error.code, WorkflowExecutionErrorCode::RuntimeFailed);
        assert_eq!(
            workflow_execution_error.message,
            "agent `assistant` execution failed: Unable to connect to the weather service"
        );

        let expected_error_context = json!({
            "messages": [],
            "total_tokens": 0,
            "input_tokens": 0,
            "output_tokens": 0,
        });

        assert_eq!(workflow_execution_error.context, Some(expected_error_context));
    }

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
            secrets: None,
            custom_tools: Vec::new(),
            tool_callback: None,
            defer_output: false,
        };

        let request_envelope = FfiRequestEnvelope::new(FfiRequest::ExecuteWorkflow(workflow_execution_request));
        let engine_ffi = EngineFfi::new();
        let response_envelope = engine_ffi.invoke(request_envelope).expect("ffi invocation should not fail");

        let WorkflowExecutionEnvelope::Succeeded { output } = response_envelope.response.expect_execute_workflow() else {
            panic!("workflow execution should succeed");
        };

        assert_eq!(output.output, Some(json!({ "greeting": "hello from ffi" })));
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
            secrets: None,
            custom_tools: vec![CustomToolDeclaration {
                name: String::from("echo"),
                description: Some(String::from("Echo tool")),
                input_schema: json!({ "type": "object" }),
                output_schema: None,
            }],
            tool_callback: None,
            defer_output: false,
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
            execution_context: None,
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
            execution_context: None,
        }));

        let response_envelope = engine_ffi
            .invoke(request_envelope)
            .expect("ffi invocation should not fail on typed envelope errors");

        let ToolInvocationEnvelope::Failed { error } = response_envelope.response.expect_invoke_tool() else {
            panic!("tool invocation should fail for unknown execution");
        };

        assert_eq!(error.code, ToolInvocationErrorCode::ToolNotFound);
    }

    #[test]
    fn trims_oldest_registered_execution_tool_declarations() {
        let custom_tool_registry = Arc::new(CustomToolRegistry::default());

        for execution_index in 0..(MAX_REGISTERED_EXECUTIONS + 4) {
            custom_tool_registry
                .register_execution_tools(&format!("execution-{execution_index}"), &[])
                .expect("registration should succeed");
        }

        let removed_execution_result = custom_tool_registry.runtime_tools_for_requested_tools("execution-0", &[], &json!({}));
        let newest_execution_result = custom_tool_registry
            .runtime_tools_for_requested_tools(&format!("execution-{}", MAX_REGISTERED_EXECUTIONS + 3), &[], &json!({}))
            .expect("newest execution declarations should still exist");

        assert!(matches!(removed_execution_result, Err(WorkflowRuntimeError::Other { message: _ })));
        assert!(newest_execution_result.is_empty());
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
