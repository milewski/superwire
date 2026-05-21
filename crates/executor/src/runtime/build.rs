use super::{value_object, ExecutorError, WorkflowExecutor};
use crate::event::ExecutorEvent;
use crate::runtime::tools::StartupMcpToolValidationContext;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use superwire_core::dsl::{parse_workflow, validate_workflow, Declaration, Workflow};
use superwire_core::mcp::{HttpMcpClientFactory, McpClientFactory, McpClientPool, McpLock, McpServerConfig};
use superwire_core::semantic::support::expression::EvaluationContext;
use superwire_core::semantic::{build_dynamic_typed_workflow_ir, build_execution_plan, WorkflowSemanticError};
use tokio::sync::mpsc;

struct RuntimeBuildContext<'a> {
    workflow_source: &'a str,
    input: &'a Value,
    secrets: &'a Value,
    event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
    mcp_client_factory: &'a dyn McpClientFactory,
}

impl RuntimeBuildContext<'_> {
    fn evaluation_context(&self) -> EvaluationContext {
        EvaluationContext {
            input_values: value_object(self.input),
            secret_values: value_object(self.secrets),
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        }
    }
}

struct McpLockDiscoveryContext<'a> {
    workflow: &'a Workflow,
    evaluation_context: &'a EvaluationContext,
    event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
    mcp_client_factory: &'a dyn McpClientFactory,
}

struct WorkflowCompilationContext<'a> {
    workflow_source: &'a str,
    workflow: Workflow,
    mcp_pool: McpClientPool,
}

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

        Self::from_workflow(WorkflowCompilationContext {
            workflow_source,
            workflow,
            mcp_pool,
        })
    }

    pub fn from_source_with_runtime_values(workflow_source: &str, input: &Value, secrets: &Value) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_and_event_sender(workflow_source, input, secrets, None)
    }

    pub fn from_source_with_runtime_values_and_mcp_client_factory(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_event_sender_and_mcp_client_factory(workflow_source, input, secrets, None, mcp_client_factory)
    }

    pub fn from_source_with_runtime_values_and_event_sender(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_event_sender_and_mcp_client_factory(
            workflow_source,
            input,
            secrets,
            event_sender,
            &HttpMcpClientFactory,
        )
    }

    fn from_source_with_runtime_values_event_sender_and_mcp_client_factory(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        let build_context = RuntimeBuildContext {
            workflow_source,
            input,
            secrets,
            event_sender,
            mcp_client_factory,
        };
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_for_output_target(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: parse_error,
                details,
            }
        })?;
        let evaluation_context = build_context.evaluation_context();
        let mcp_lock = Self::discover_mcp_lock_with_context(McpLockDiscoveryContext {
            workflow: &workflow,
            evaluation_context: &evaluation_context,
            event_sender: build_context.event_sender,
            mcp_client_factory: build_context.mcp_client_factory,
        })?;

        log::debug!("discovered MCP schemas using runtime values: servers={}", mcp_lock.servers.len());
        mcp_lock.apply_to_workflow(&mut workflow);
        let prompt_binding_errors = mcp_lock.validate_prompt_import_bindings(&workflow);

        if !prompt_binding_errors.is_empty() {
            return Err(ExecutorError::Other {
                message: prompt_binding_errors.join("; "),
            });
        }

        let mcp_pool =
            McpClientPool::from_workflow_with_context_and_factory(&workflow, &evaluation_context, build_context.mcp_client_factory)
                .map_err(|error| ExecutorError::Other {
                    message: error.to_string(),
                })?;

        let executor = Self::from_workflow(WorkflowCompilationContext {
            workflow_source: build_context.workflow_source,
            workflow,
            mcp_pool,
        })?;
        executor.validate_startup_mcp_tool_calls(StartupMcpToolValidationContext {
            evaluation_context: &evaluation_context,
            event_sender: build_context.event_sender,
        })?;

        Ok(executor)
    }

    fn discover_mcp_lock_with_context(discovery_context: McpLockDiscoveryContext<'_>) -> Result<McpLock, ExecutorError> {
        let mut mcp_lock = McpLock::empty();

        for declaration in discovery_context.workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, discovery_context.evaluation_context)
                .map_err(|error| ExecutorError::Other {
                    message: error.to_string(),
                })?;
            let started_at = Instant::now();

            if let Some(sender) = discovery_context.event_sender {
                let _ = sender.try_send(ExecutorEvent::mcp_tool_schema_fetch_started(server_config.name.clone()));
            }

            log::debug!("discovering MCP tools from runtime server config: {}", server_config.name);
            let server_lock = match discovery_context
                .mcp_client_factory
                .client_for_config(server_config.clone())
                .and_then(|client| client.list_tools())
            {
                Ok(server_lock) => server_lock,
                Err(error) => {
                    if let Some(sender) = discovery_context.event_sender {
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

            if let Some(sender) = discovery_context.event_sender {
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

    fn from_workflow(compilation_context: WorkflowCompilationContext<'_>) -> Result<Self, ExecutorError> {
        log::debug!("validating workflow after schema discovery");
        let validation_report = validate_workflow(&compilation_context.workflow);

        if validation_report.has_issues() {
            let issues = validation_report.render_for_output_target(Some(compilation_context.workflow_source), "<workflow>");

            return Err(WorkflowSemanticError::InvalidWorkflow { issues }.into());
        }

        let typed_workflow_ir = build_dynamic_typed_workflow_ir(&compilation_context.workflow)
            .map_err(|error| error.into_compilation_diagnostic(&compilation_context.workflow, "<workflow>"))?;
        let execution_plan = build_execution_plan(&compilation_context.workflow, &typed_workflow_ir)
            .map_err(|error| error.into_compilation_diagnostic(&compilation_context.workflow, "<workflow>"))?;

        log::info!(
            "workflow planned: agents={}, tools={}, agent_order={}",
            execution_plan.planned_agents.len(),
            execution_plan.tools.len(),
            execution_plan.agent_execution_order.len()
        );

        Ok(Self {
            workflow: compilation_context.workflow,
            execution_plan,
            mcp_pool: compilation_context.mcp_pool,
        })
    }
}
