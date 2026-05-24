mod agent;
mod build;
pub mod cache;
mod configuration;
pub mod error;
mod execution;
mod for_loop;
mod mcp;
mod schema;
pub mod state;
mod tools;

pub(in crate::runtime) use agent::AgentRunContext;
pub use cache::{
    AgentCacheConfig, AgentCacheDriver, AgentCacheOptions, AgentCacheSession, AgentCacheTimeToLive, RedisAgentCacheConfig,
    DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
};
pub(in crate::runtime) use configuration::RuntimeValidationContext;
pub use error::ExecutorError;
pub(in crate::runtime) use schema::value_object;

use crate::event::ExecutorEvent;
use crate::model::ToolCallTracker;
use crate::runtime::mcp::normalize_prompt;
use crate::runtime::state::RuntimeState;
use crate::runtime::tools::ExpressionMcpExecutionPlanExt;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use superwire_core::dsl::{
    AgentProperty, Declaration, Expression, McpPromptImportDeclaration, McpResourceImportDeclaration, McpServerDeclaration, Workflow,
};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::{ExecutionPlan, WorkflowExecutionGraph};
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Clone)]
pub(in crate::runtime) struct CompletedAgentExecution {
    pub(in crate::runtime) agent_name: String,
    pub(in crate::runtime) output: Value,
    pub(in crate::runtime) context: Value,
}

impl CompletedAgentExecution {
    pub(in crate::runtime) fn apply_to_runtime_state(self, runtime_state: &mut RuntimeState) {
        runtime_state.insert_agent_result(self.agent_name, self.output, self.context);
    }
}

#[derive(Debug)]
pub struct WorkflowExecutor {
    workflow: Workflow,
    execution_plan: ExecutionPlan,
    mcp_pool: McpClientPool,
    lookups: WorkflowExecutorLookups,
}

#[derive(Debug, Clone)]
struct WorkflowExecutorLookups {
    mcp_servers: HashMap<String, McpServerDeclaration>,
    default_mcp_server: Option<McpServerDeclaration>,
    resource_imports: HashMap<String, McpResourceImportDeclaration>,
    prompt_imports: HashMap<String, McpPromptImportDeclaration>,
}

impl WorkflowExecutorLookups {
    fn from_workflow(workflow: &Workflow) -> Self {
        let mut mcp_servers = HashMap::new();
        let mut default_mcp_server = None;

        for declaration in workflow.declarations() {
            if let Declaration::McpServer(mcp_server_declaration) = declaration {
                if default_mcp_server.is_none() {
                    default_mcp_server = Some(mcp_server_declaration.clone());
                }

                mcp_servers
                    .entry(mcp_server_declaration.name.clone())
                    .or_insert_with(|| mcp_server_declaration.clone());
            }
        }

        let mut resource_imports = HashMap::new();
        let mut prompt_imports = HashMap::new();

        for resource_import_declaration in workflow.resource_imports() {
            resource_imports
                .entry(resource_import_declaration.name.clone())
                .or_insert_with(|| resource_import_declaration.clone());
        }

        for prompt_import_declaration in workflow.prompt_imports() {
            prompt_imports
                .entry(prompt_import_declaration.name.clone())
                .or_insert_with(|| prompt_import_declaration.clone());
        }

        Self {
            mcp_servers,
            default_mcp_server,
            resource_imports,
            prompt_imports,
        }
    }

    fn mcp_server(&self, server_name: &str) -> Option<&McpServerDeclaration> {
        self.mcp_servers.get(server_name)
    }

    fn default_mcp_server(&self) -> Option<&McpServerDeclaration> {
        self.default_mcp_server.as_ref()
    }

    fn resource_import(&self, resource_name: &str) -> Option<&McpResourceImportDeclaration> {
        self.resource_imports.get(resource_name)
    }

    fn prompt_import(&self, prompt_name: &str) -> Option<&McpPromptImportDeclaration> {
        self.prompt_imports.get(prompt_name)
    }
}

#[derive(Debug, Clone)]
pub(in crate::runtime) struct AgentExecutionContext {
    pub(in crate::runtime) event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    pub(in crate::runtime) import_context: String,
    pub(in crate::runtime) tool_call_tracker: ToolCallTracker,
    pub(in crate::runtime) runtime_concurrency_limiter: RuntimeConcurrencyLimiter,
    pub(in crate::runtime) cache_options: AgentCacheOptions,
}

#[derive(Debug, Clone)]
pub(in crate::runtime) struct RuntimeConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl RuntimeConcurrencyLimiter {
    pub(in crate::runtime) fn new(max_concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
        }
    }

    pub(in crate::runtime) async fn run<Output, ExecutionFuture>(&self, execution_future: ExecutionFuture) -> Result<Output, ExecutorError>
    where
        ExecutionFuture: Future<Output = Result<Output, ExecutorError>>,
    {
        let _permit = self.semaphore.clone().acquire_owned().await.map_err(|error| ExecutorError::Other {
            message: format!("failed to acquire runtime concurrency permit: {error}"),
        })?;

        execution_future.await
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::runtime) struct ToolCallExecutionContext<'a> {
    pub(in crate::runtime) evaluation_context: &'a EvaluationContext,
    pub(in crate::runtime) event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
    pub(in crate::runtime) tool_call_tracker: &'a ToolCallTracker,
}

impl<'a> ToolCallExecutionContext<'a> {
    pub(in crate::runtime) fn new(
        evaluation_context: &'a EvaluationContext,
        event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &'a ToolCallTracker,
    ) -> Self {
        Self {
            evaluation_context,
            event_sender,
            tool_call_tracker,
        }
    }
}

impl WorkflowExecutor {
    #[must_use]
    pub fn agent_execution_order(&self) -> Vec<String> {
        self.execution_plan.agent_execution_order.clone()
    }

    #[must_use]
    pub fn mcp_imports(&self) -> &[superwire_core::semantic::PlannedMcpImport] {
        &self.execution_plan.mcp_imports
    }

    #[must_use]
    pub fn execution_graph(&self) -> WorkflowExecutionGraph {
        self.execution_plan.execution_graph(&self.workflow)
    }

    pub fn planned_execution_steps(&self, input: &Value, secrets: &Value, max_concurrency: usize) -> Result<Value, ExecutorError> {
        let runtime_configuration = self.resolve_runtime_configuration(RuntimeValidationContext { input, secrets })?;
        let runtime_state = RuntimeState::new(runtime_configuration.input_values, runtime_configuration.secret_values);
        let evaluation_context = runtime_state.evaluation_context();
        let mut steps = Vec::new();

        let dynamic_calls = self.planned_workflow_dynamic_calls(&evaluation_context)?;

        if !dynamic_calls.is_empty() {
            steps.push(serde_json::json!({
                "type": "workflow_dynamic",
                "calls": dynamic_calls,
            }));
        }

        for execution_batch in self.execution_plan.agent_execution_batches()? {
            let planned_agents = execution_batch
                .iter()
                .map(|agent_name| self.planned_agent_step(agent_name, &evaluation_context))
                .collect::<Result<Vec<_>, _>>()?;

            if planned_agents.len() == 1 || max_concurrency <= 1 {
                steps.extend(planned_agents);
            } else {
                steps.push(serde_json::json!({ "parallel": planned_agents }));
            }
        }

        let output_calls = self.planned_output_calls(&evaluation_context)?;

        steps.push(serde_json::json!({
            "type": "workflow_output",
            "calls": output_calls,
        }));

        Ok(Value::Array(steps))
    }

    fn planned_workflow_dynamic_calls(&self, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError> {
        let mut calls = Vec::new();

        for declaration in self.workflow.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                calls.extend(dynamic_field.value.planned_mcp_calls(self, evaluation_context)?);
            }
        }

        Ok(calls)
    }

    fn planned_output_calls(&self, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError> {
        let mut calls = Vec::new();

        for output_field in &self.execution_plan.output_declaration.fields {
            calls.extend(output_field.value.planned_mcp_calls(self, evaluation_context)?);
        }

        Ok(calls)
    }

    fn planned_agent_step(&self, agent_name: &str, evaluation_context: &EvaluationContext) -> Result<Value, ExecutorError> {
        let planned_agent = self
            .execution_plan
            .planned_agents
            .get(agent_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("planned agent `{agent_name}` is missing"),
            })?;
        let mut dynamic_calls = Vec::new();

        for agent_property in &planned_agent.declaration.properties {
            let AgentProperty::Dynamic(dynamic_block) = agent_property else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                dynamic_calls.extend(dynamic_field.value.planned_mcp_calls(self, evaluation_context)?);
            }
        }

        Ok(serde_json::json!({
            "type": "agent",
            "agent_name": agent_name,
            "dependencies": planned_agent.dependencies,
            "iteration_count": planned_agent.iteration_count(evaluation_context)?,
            "dynamic_calls": dynamic_calls,
            "available_mcp_calls": self.planned_agent_available_mcp_calls(planned_agent, evaluation_context)?,
        }))
    }

    fn evaluate_runtime_expression(
        &self,
        expression: &Expression,
        tool_call_execution_context: ToolCallExecutionContext<'_>,
        context: &str,
    ) -> Result<Value, ExecutorError> {
        match expression {
            Expression::StringLiteral(string_literal) => Ok(Value::String(string_literal.clone())),
            Expression::StringTemplate(string_template) => {
                let mut rendered_template = String::new();

                for string_template_part in &string_template.parts {
                    match string_template_part {
                        superwire_core::dsl::StringTemplatePart::Text(template_text) => rendered_template.push_str(template_text),
                        superwire_core::dsl::StringTemplatePart::Interpolation(interpolation_expression) => {
                            let interpolation_value =
                                self.evaluate_runtime_expression(interpolation_expression, tool_call_execution_context, context)?;
                            rendered_template.push_str(&normalize_prompt(&interpolation_value));
                        }
                    }
                }

                Ok(Value::String(rendered_template))
            }
            Expression::NumberLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral
            | Expression::Reference(_)
            | Expression::FunctionCall(_)
            | Expression::Asset(_)
            | Expression::VariantProjection(_)
            | Expression::Match(_) => Ok(evaluate_expression(
                expression,
                tool_call_execution_context.evaluation_context,
                context,
            )?),
            Expression::NullFallback(null_fallback) => {
                let value = self.evaluate_runtime_expression(&null_fallback.value, tool_call_execution_context, context)?;

                if value.is_null() {
                    return self.evaluate_runtime_expression(&null_fallback.fallback, tool_call_execution_context, context);
                }

                Ok(value)
            }
            Expression::ToolCall(tool_call) => self.execute_deterministic_tool_call(tool_call, tool_call_execution_context),
            Expression::McpCall(mcp_call) => self.execute_mcp_call(mcp_call, tool_call_execution_context.into()),
            Expression::ArrayLiteral(array_items) => {
                let mut evaluated_items = Vec::with_capacity(array_items.len());

                for array_item in array_items {
                    evaluated_items.push(self.evaluate_runtime_expression(array_item, tool_call_execution_context, context)?);
                }

                Ok(Value::Array(evaluated_items))
            }
            Expression::ObjectLiteral(object_fields) => {
                let mut evaluated_fields = Map::new();

                for object_field in object_fields {
                    let field_value = self.evaluate_runtime_expression(&object_field.value, tool_call_execution_context, context)?;
                    evaluated_fields.insert(object_field.name.clone(), field_value);
                }

                Ok(Value::Object(evaluated_fields))
            }
        }
    }
}

trait PlannedAgentRuntimePlanningExt {
    fn iteration_count(&self, evaluation_context: &EvaluationContext) -> Result<usize, ExecutorError>;
}

impl PlannedAgentRuntimePlanningExt for superwire_core::semantic::PlannedAgent {
    fn iteration_count(&self, evaluation_context: &EvaluationContext) -> Result<usize, ExecutorError> {
        let Some(agent_for_loop) = &self.declaration.for_loop else {
            return Ok(1);
        };
        let Ok(iterable_value) = evaluate_expression(
            &agent_for_loop.iterable,
            evaluation_context,
            &format!("for-loop iterable for agent `{}`", self.name),
        ) else {
            return Ok(1);
        };

        Ok(iterable_value.as_array().map_or(1, Vec::len))
    }
}
