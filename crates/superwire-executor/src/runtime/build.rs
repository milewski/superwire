use super::{ExecutorError, WorkflowExecutor, WorkflowExecutorLookups};
use crate::model::ExecutorEventSenderExt;
use crate::runtime::tools::StartupMcpToolValidationContext;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::Instant;
use superwire_dsl::{parse_workflow, validate_workflow, Declaration, ValidationReport, Workflow};
use superwire_mcp::{HttpMcpClientFactory, McpClientFactory, McpClientPool, McpClientRequestScope, McpLock, McpServerConfig};
use superwire_protocol::event::ExecutorEvent;
use superwire_semantic::support::expression::EvaluationContext;
use superwire_semantic::support::types::value_kind_name;
use superwire_semantic::{
    build_dynamic_typed_workflow_ir, build_execution_plan, ExecutionPlan, WorkflowSemanticError, WorkflowSemanticIndex,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeInputValidation {
    Required,
    Skipped,
}

struct RuntimeBuildContext<'a> {
    workflow_source: &'a str,
    input: &'a Value,
    secrets: &'a Value,
    input_validation: RuntimeInputValidation,
    event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
    mcp_client_factory: &'a dyn McpClientFactory,
}

impl RuntimeBuildContext<'_> {
    fn evaluation_context(&self) -> Result<EvaluationContext, ExecutorError> {
        let input_values = match self.input {
            Value::Object(input_values) => input_values.clone(),
            Value::Null => Map::new(),
            invalid_input => {
                return Err(ExecutorError::InputTypeMismatch {
                    expected: "input object or null".to_string(),
                    found: value_kind_name(invalid_input).to_string(),
                });
            }
        };

        let secret_values = match self.secrets {
            Value::Object(secret_values) => secret_values.clone(),
            Value::Null => Map::new(),
            invalid_secrets => {
                return Err(ExecutorError::InputTypeMismatch {
                    expected: "secrets object or null".to_string(),
                    found: value_kind_name(invalid_secrets).to_string(),
                });
            }
        };

        Ok(EvaluationContext {
            input_values,
            secret_values,
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        })
    }

    fn validate_runtime_values(&self, semantic_index: &WorkflowSemanticIndex) -> Result<(), ExecutorError> {
        if self.input_validation == RuntimeInputValidation::Required {
            if let Some(input_type) = semantic_index.input_type() {
                if self.input.is_null() {
                    return Err(ExecutorError::InputValueMismatch {
                        message: format!("workflow declares an `input` block, but no input object was provided; expected {input_type}"),
                    });
                }

                if !self.input.is_object() {
                    return Err(ExecutorError::InputValueMismatch {
                        message: format!(
                            "declared `input` block expects object matching {input_type}, found {}",
                            value_kind_name(self.input)
                        ),
                    });
                }

                input_type
                    .validate_value(self.input)
                    .map_err(|message| ExecutorError::InputValueMismatch {
                        message: format!("declared `input` block expects {input_type}: {message}"),
                    })?;
            } else if !self.input.is_null() && !self.input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err(ExecutorError::InputTypeMismatch {
                    expected: "no input".to_string(),
                    found: value_kind_name(self.input).to_string(),
                });
            }
        }

        if let Some(secrets_type) = semantic_index.secrets_type() {
            if self.secrets.is_null() {
                return Err(ExecutorError::SecretValueMismatch {
                    message: format!("workflow declares a `secrets` block, but no secrets object was provided; expected {secrets_type}"),
                });
            }

            secrets_type
                .validate_value(self.secrets)
                .map_err(|message| ExecutorError::SecretValueMismatch {
                    message: format!("declared `secrets` block expects {secrets_type}: {message}"),
                })?;

            if !self.secrets.is_object() {
                return Err(ExecutorError::SecretValueMismatch {
                    message: format!(
                        "declared `secrets` block expects object matching {secrets_type}, found {}",
                        value_kind_name(self.secrets)
                    ),
                });
            }
        } else if !self.secrets.is_null() && !self.secrets.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(ExecutorError::InputTypeMismatch {
                expected: "no secrets".to_string(),
                found: value_kind_name(self.secrets).to_string(),
            });
        }

        Ok(())
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

struct RuntimeWorkflowPlan {
    workflow: Workflow,
    execution_plan: ExecutionPlan,
}

impl RuntimeWorkflowPlan {
    fn compile(workflow_source: &str, workflow: Workflow) -> Result<Self, ExecutorError> {
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

        Ok(Self { workflow, execution_plan })
    }

    fn into_executor(self, mcp_pool: McpClientPool) -> WorkflowExecutor {
        let lookups = WorkflowExecutorLookups::from_workflow(&self.workflow);

        WorkflowExecutor {
            workflow: self.workflow,
            execution_plan: self.execution_plan,
            mcp_pool,
            lookups,
        }
    }
}

impl WorkflowExecutor {
    pub fn from_source(workflow_source: &str) -> Result<Self, ExecutorError> {
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_for_output_target(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: Box::new(parse_error),
                details,
            }
        })?;
        let mcp_lock = McpLock::discover_from_workflow(&workflow)
            .map_err(|error| ExecutorError::internal_with_source(error.public_message(), error))?;
        mcp_lock.apply_to_workflow(&mut workflow);
        let prompt_binding_errors = mcp_lock.validate_prompt_import_bindings(&workflow);

        if !prompt_binding_errors.is_empty() {
            return Err(ExecutorError::Other {
                message: prompt_binding_errors.join("; "),
            });
        }

        let mcp_pool = McpClientPool::from_workflow(&workflow)
            .map_err(|error| ExecutorError::mcp_with_source(None, None, None, error.public_message(), error))?;

        Self::from_workflow(WorkflowCompilationContext {
            workflow_source,
            workflow,
            mcp_pool,
        })
    }

    pub fn from_source_with_runtime_values(workflow_source: &str, input: &Value, secrets: &Value) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_and_event_sender(workflow_source, input, secrets, None)
    }

    pub(crate) fn from_source_for_validation_with_mcp_client_factory(
        workflow_source: &str,
        secrets: &Value,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_event_sender_and_mcp_client_factory(
            workflow_source,
            &Value::Null,
            secrets,
            RuntimeInputValidation::Skipped,
            None,
            mcp_client_factory,
        )
    }

    pub fn from_source_with_runtime_values_and_mcp_client_factory(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_event_sender_and_mcp_client_factory(
            workflow_source,
            input,
            secrets,
            RuntimeInputValidation::Required,
            None,
            mcp_client_factory,
        )
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
            RuntimeInputValidation::Required,
            event_sender,
            &HttpMcpClientFactory,
        )
    }

    pub(crate) fn from_source_with_runtime_values_and_event_sender_and_mcp_client_factory(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        Self::from_source_with_runtime_values_event_sender_and_mcp_client_factory(
            workflow_source,
            input,
            secrets,
            RuntimeInputValidation::Required,
            event_sender,
            mcp_client_factory,
        )
    }

    fn from_source_with_runtime_values_event_sender_and_mcp_client_factory(
        workflow_source: &str,
        input: &Value,
        secrets: &Value,
        input_validation: RuntimeInputValidation,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<Self, ExecutorError> {
        let build_context = RuntimeBuildContext {
            workflow_source,
            input,
            secrets,
            input_validation,
            event_sender,
            mcp_client_factory,
        };
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_for_output_target(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: Box::new(parse_error),
                details,
            }
        })?;
        let mut preflight_validation_report = ValidationReport::default();
        let semantic_index = WorkflowSemanticIndex::build_for_validation(&workflow, &mut preflight_validation_report);
        build_context.validate_runtime_values(&semantic_index)?;
        let mut evaluation_context = build_context.evaluation_context()?;
        evaluation_context.evaluate_available_workflow_dynamic_bindings(&workflow);
        let mcp_request_scope = McpClientRequestScope::from_workflow(build_context.mcp_client_factory, &workflow, &evaluation_context)
            .map_err(|error| ExecutorError::mcp_with_source(None, None, None, error.public_message(), error))?;
        let mcp_lock = Self::discover_mcp_lock_with_context(McpLockDiscoveryContext {
            workflow: &workflow,
            evaluation_context: &evaluation_context,
            event_sender: build_context.event_sender,
            mcp_client_factory: &mcp_request_scope,
        })?;

        log::debug!("discovered MCP schemas using runtime values: servers={}", mcp_lock.servers.len());
        mcp_lock.apply_to_workflow(&mut workflow);
        let prompt_binding_errors = mcp_lock.validate_prompt_import_bindings(&workflow);

        if !prompt_binding_errors.is_empty() {
            return Err(ExecutorError::Other {
                message: prompt_binding_errors.join("; "),
            });
        }

        let mcp_pool = McpClientPool::from_workflow_with_context_and_factory(&workflow, &evaluation_context, &mcp_request_scope)
            .map_err(|error| ExecutorError::mcp_with_source(None, None, None, error.public_message(), error))?;

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
        let mut server_configs = Vec::new();

        for declaration in discovery_context.workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration_with_endpoint_validator(
                mcp_server_declaration,
                discovery_context.evaluation_context,
                |server_name, endpoint| discovery_context.mcp_client_factory.validate_endpoint(server_name, endpoint),
            )
            .map_err(|error| {
                ExecutorError::mcp_with_source(None, Some(mcp_server_declaration.name.clone()), None, error.public_message(), error)
            })?;

            server_configs.push(server_config);
        }

        let mut mcp_lock = McpLock::empty();

        for server_config in server_configs {
            let started_at = Instant::now();

            if let Some(sender) = discovery_context.event_sender {
                sender.try_send_observed(ExecutorEvent::mcp_tool_schema_fetch_started(server_config.name.clone()));
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
                        sender.try_send_observed(ExecutorEvent::mcp_tool_schema_fetch_failed(
                            server_config.name.clone(),
                            started_at.elapsed(),
                        ));
                    }

                    return Err(ExecutorError::mcp_with_source(
                        None,
                        Some(server_config.name),
                        None,
                        "MCP tool schema discovery failed",
                        error,
                    ));
                }
            };
            let tool_count = server_lock.tools.len();

            if let Some(sender) = discovery_context.event_sender {
                sender.try_send_observed(ExecutorEvent::mcp_tool_schema_fetch_completed(
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
        let runtime_plan = RuntimeWorkflowPlan::compile(compilation_context.workflow_source, compilation_context.workflow)?;

        log::info!(
            "workflow planned: agents={}, tools={}, agent_order={}",
            runtime_plan.execution_plan.planned_agents.len(),
            runtime_plan.execution_plan.tools.len(),
            runtime_plan.execution_plan.agent_execution_order.len()
        );

        Ok(runtime_plan.into_executor(compilation_context.mcp_pool))
    }
}
