use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use engine_ai_agent::{DynamicTool, ToolError};
use engine_ai_core::runtime::WorkflowRuntimeError;
use schemars::Schema;
use serde_json::Value;

use super::{CustomToolHandler, MAX_REGISTERED_EXECUTIONS};
use crate::types::{
    CustomToolDeclaration, ToolCallbackConfig, ToolInvocationError, ToolInvocationErrorCode, ToolInvocationPayload, WorkflowExecutionError,
};

#[derive(Default)]
pub(super) struct CustomToolRegistry {
    handlers_by_name: RwLock<HashMap<String, CustomToolHandler>>,
    declarations_by_execution: RwLock<HashMap<String, HashMap<String, CustomToolDeclaration>>>,
    registration_order: RwLock<VecDeque<String>>,
}

impl CustomToolRegistry {
    pub(super) fn register_handler(&self, tool_name: String, custom_tool_handler: CustomToolHandler) {
        let mut handlers_by_name = self
            .handlers_by_name
            .write()
            .expect("custom tool handler registry lock should not be poisoned");

        handlers_by_name.insert(tool_name, custom_tool_handler);
    }

    pub(super) fn has_handler(&self, tool_name: &str) -> bool {
        let handlers_by_name = self
            .handlers_by_name
            .read()
            .expect("custom tool handler registry lock should not be poisoned");

        handlers_by_name.contains_key(tool_name)
    }

    pub(super) fn ensure_callback_handlers(
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

    pub(super) fn register_execution_tools(
        &self,
        execution_id: &str,
        custom_tools: &[CustomToolDeclaration],
    ) -> Result<(), WorkflowExecutionError> {
        let mut declarations_by_name = HashMap::new();

        for custom_tool in custom_tools {
            if declarations_by_name.insert(custom_tool.name.clone(), custom_tool.clone()).is_some() {
                return Err(WorkflowExecutionError::tool_invocation_failed(
                    format!("duplicate custom tool declaration `{}`", custom_tool.name),
                    None,
                ));
            }
        }

        self.insert_execution_declarations(execution_id.to_string(), declarations_by_name);

        Ok(())
    }

    pub(super) async fn invoke(&self, tool_invocation_payload: ToolInvocationPayload) -> Result<Value, ToolInvocationError> {
        let custom_tool_handler = self.resolve_handler(&tool_invocation_payload)?;

        custom_tool_handler(tool_invocation_payload).await
    }

    pub(super) fn runtime_tools_for_requested_tools(
        self: &Arc<Self>,
        execution_id: &str,
        requested_tools: &[engine_ai_core::runtime::RequestedAgentTool],
        workflow_input: &Value,
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

        for requested_tool in requested_tools {
            let Some(declared_tool) = declarations_by_name.get(&requested_tool.name) else {
                return Err(WorkflowRuntimeError::InvalidAgentProperty {
                    agent_name: String::from("ffi"),
                    property: String::from("tools"),
                    message: format!(
                        "requested custom tool `tool.{}` was not declared for execution `{execution_id}`",
                        requested_tool.name
                    ),
                });
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
            let bound_arguments = Value::Object(requested_tool.bound_arguments.clone());
            let workflow_input = workflow_input.clone();

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
                    let bound_arguments = bound_arguments.clone();
                    let workflow_input = workflow_input.clone();

                    async move {
                        let execution_context = serde_json::json!({
                            "workflow_input": workflow_input,
                            "bound_arguments": bound_arguments,
                        });

                        let tool_invocation_payload =
                            ToolInvocationPayload::from_runtime_request(execution_id, tool_name, arguments, Some(execution_context));

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

    fn insert_execution_declarations(&self, execution_id: String, declarations_by_name: HashMap<String, CustomToolDeclaration>) {
        let mut declarations_by_execution = self
            .declarations_by_execution
            .write()
            .expect("custom tool declaration registry lock should not be poisoned");
        let mut registration_order = self
            .registration_order
            .write()
            .expect("custom tool registration order lock should not be poisoned");

        if let Some(existing_execution_position) = registration_order
            .iter()
            .position(|registered_execution_id| registered_execution_id == &execution_id)
        {
            registration_order.remove(existing_execution_position);
        }

        registration_order.push_back(execution_id.clone());
        declarations_by_execution.insert(execution_id, declarations_by_name);

        self.trim_registered_executions(&mut declarations_by_execution, &mut registration_order);
    }

    fn trim_registered_executions(
        &self,
        declarations_by_execution: &mut HashMap<String, HashMap<String, CustomToolDeclaration>>,
        registration_order: &mut VecDeque<String>,
    ) {
        while declarations_by_execution.len() > MAX_REGISTERED_EXECUTIONS {
            let Some(oldest_execution_id) = registration_order.pop_front() else {
                break;
            };

            declarations_by_execution.remove(&oldest_execution_id);
        }
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
