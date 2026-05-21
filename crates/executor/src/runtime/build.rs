use super::{value_object, ExecutorError, WorkflowExecutor};
use crate::event::ExecutorEvent;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use superwire_core::dsl::{parse_workflow, validate_workflow, Declaration, Workflow};
use superwire_core::mcp::{McpClient, McpClientPool, McpLock, McpServerConfig};
use superwire_core::semantic::support::expression::EvaluationContext;
use superwire_core::semantic::{build_dynamic_typed_workflow_ir, build_execution_plan, WorkflowSemanticError};
use tokio::sync::mpsc;

impl WorkflowExecutor {
    pub fn from_source(workflow_source: &str) -> Result<Self, ExecutorError> {
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_for_output_target(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: parse_error,
                details,
            }
        })?;
        let mcp_lock = McpLock::discover_from_workflow(&workflow).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;
        mcp_lock.apply_to_workflow(&mut workflow);
        let prompt_binding_errors = mcp_lock.validate_prompt_import_bindings(&workflow);

        if !prompt_binding_errors.is_empty() {
            return Err(ExecutorError::Other {
                message: prompt_binding_errors.join("; "),
            });
        }

        let mcp_pool = McpClientPool::from_workflow(&workflow).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;

        Self::from_workflow(workflow_source, workflow, mcp_pool)
    }

    pub fn from_source_with_runtime_values(workflow_source: &str, input: &Value, secrets: &Value) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_and_event_sender(workflow_source, input, secrets, None)
    }

    pub fn from_source_with_runtime_values_and_event_sender(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Self, ExecutorError> {
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_for_output_target(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: parse_error,
                details,
            }
        })?;
        let evaluation_context = EvaluationContext {
            input_values: value_object(input),
            secret_values: value_object(secrets),
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        };
        let mcp_lock = Self::discover_mcp_lock_with_context(&workflow, &evaluation_context, event_sender)?;

        log::debug!("discovered MCP schemas using runtime values: servers={}", mcp_lock.servers.len());
        mcp_lock.apply_to_workflow(&mut workflow);
        let prompt_binding_errors = mcp_lock.validate_prompt_import_bindings(&workflow);

        if !prompt_binding_errors.is_empty() {
            return Err(ExecutorError::Other {
                message: prompt_binding_errors.join("; "),
            });
        }

        let mcp_pool = McpClientPool::from_workflow_with_context(&workflow, &evaluation_context).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;

        let executor = Self::from_workflow(workflow_source, workflow, mcp_pool)?;
        executor.validate_startup_mcp_tool_calls(&evaluation_context, event_sender)?;

        Ok(executor)
    }

    fn discover_mcp_lock_with_context(
        workflow: &Workflow,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<McpLock, ExecutorError> {
        let mut mcp_lock = McpLock::empty();

        for declaration in workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context).map_err(|error| {
                ExecutorError::Other {
                    message: error.to_string(),
                }
            })?;
            let started_at = Instant::now();

            if let Some(sender) = event_sender {
                let _ = sender.try_send(ExecutorEvent::mcp_tool_schema_fetch_started(server_config.name.clone()));
            }

            log::debug!("discovering MCP tools from runtime server config: {}", server_config.name);
            let server_lock = match McpClient::new(server_config.clone()).list_tools() {
                Ok(server_lock) => server_lock,
                Err(error) => {
                    if let Some(sender) = event_sender {
                        let _ = sender.try_send(ExecutorEvent::mcp_tool_schema_fetch_failed(
                            server_config.name,
                            Value::String(error.to_string()),
                            started_at.elapsed(),
                        ));
                    }

                    return Err(ExecutorError::Other {
                        message: error.to_string(),
                    });
                }
            };
            let tool_count = server_lock.tools.len();

            if let Some(sender) = event_sender {
                let _ = sender.try_send(ExecutorEvent::mcp_tool_schema_fetch_completed(
                    server_config.name.clone(),
                    tool_count,
                    started_at.elapsed(),
                ));
            }

            mcp_lock.servers.insert(server_config.name, server_lock);
        }

        Ok(mcp_lock)
    }

    fn from_workflow(workflow_source: &str, workflow: Workflow, mcp_pool: McpClientPool) -> Result<Self, ExecutorError> {
        log::debug!("validating workflow after schema discovery");
        let validation_report = validate_workflow(&workflow);

        if validation_report.has_issues() {
            let issues = validation_report.render_for_output_target(Some(workflow_source), "<workflow>");

            return Err(WorkflowSemanticError::InvalidWorkflow { issues }.into());
        }

        let typed_workflow_ir =
            build_dynamic_typed_workflow_ir(&workflow).map_err(|error| error.into_compilation_diagnostic(&workflow, "<workflow>"))?;
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir)
            .map_err(|error| error.into_compilation_diagnostic(&workflow, "<workflow>"))?;

        log::info!(
            "workflow planned: agents={}, tools={}, agent_order={}",
            execution_plan.planned_agents.len(),
            execution_plan.tools.len(),
            execution_plan.agent_execution_order.len()
        );

        Ok(Self {
            workflow,
            execution_plan,
            mcp_pool,
        })
    }
}
