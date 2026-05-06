pub mod error;
pub mod state;

pub use error::ExecutorError;

use crate::api::ModelResponseFormat;
use crate::event::ExecutorEvent;
use crate::model::{
    normalize_mcp_tool_result, ModelProvider, ModelRequest, ModelToolDefinition, ModelToolSource, ToolCallLimitScope, ToolCallTracker,
};
use crate::runtime::state::RuntimeState;
use futures::future::try_join_all;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use superwire_core::dsl::{
    parse_workflow, validate_workflow, AgentExpressionPropertyName, AgentForLoopPattern, AgentProperty, AgentResponseFormat, Declaration,
    Expression, ObjectField, Reference, ReferenceKeyword, ToolCall, ToolSource, Workflow,
};
use superwire_core::mcp::{McpClientPool, McpLock, McpServerConfig};
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::support::provider::ProviderConfig;
use superwire_core::semantic::support::types::{validate_value_against_type, value_kind_name, workflow_type_to_json_schema, WorkflowType};
use superwire_core::semantic::{
    build_dynamic_typed_workflow_ir, build_execution_plan, ExecutionPlan, PlannedAgent, TypedToolIr, WorkflowSemanticError,
};
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Clone)]
struct CompletedAgentExecution {
    agent_name: String,
    output: Value,
    context: Value,
}

impl CompletedAgentExecution {
    fn apply_to_runtime_state(self, runtime_state: &mut RuntimeState) {
        runtime_state.insert_agent_result(self.agent_name, self.output, self.context);
    }
}

#[derive(Debug)]
pub struct WorkflowExecutor {
    workflow: Workflow,
    execution_plan: ExecutionPlan,
    mcp_pool: McpClientPool,
}

#[derive(Debug, Clone)]
struct AgentExecutionContext {
    event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    import_context: String,
    tool_call_tracker: ToolCallTracker,
}

impl WorkflowExecutor {
    fn agent_response_format(planned_agent: &PlannedAgent) -> ModelResponseFormat {
        match planned_agent.declaration.response_format() {
            Some(AgentResponseFormat::Auto) => ModelResponseFormat::Auto,
            Some(AgentResponseFormat::JsonSchema) => ModelResponseFormat::JsonSchema,
            Some(AgentResponseFormat::JsonObject) => ModelResponseFormat::JsonObject,
            Some(AgentResponseFormat::InstructionOnly) => ModelResponseFormat::InstructionOnly,
            None => ModelResponseFormat::Auto,
        }
    }

    pub fn from_source(workflow_source: &str) -> Result<Self, ExecutorError> {
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_with_source(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: parse_error,
                details,
            }
        })?;
        let mcp_lock = McpLock::discover_from_workflow(&workflow).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;
        mcp_lock.apply_to_workflow(&mut workflow);
        let mcp_pool = McpClientPool::from_workflow(&workflow).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;

        Self::from_workflow(workflow_source, workflow, mcp_pool)
    }

    pub fn from_source_with_runtime_values(workflow_source: &str, input: &Value, secrets: &Value) -> Result<Self, ExecutorError> {
        let mut workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_with_source(workflow_source, "<workflow>");

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
        let mcp_lock =
            McpLock::discover_from_workflow_with_context(&workflow, &evaluation_context).map_err(|error| ExecutorError::Other {
                message: error.to_string(),
            })?;

        log::debug!("discovered MCP schemas using runtime values: servers={}", mcp_lock.servers.len());
        mcp_lock.apply_to_workflow(&mut workflow);
        let mcp_pool = McpClientPool::from_workflow_with_context(&workflow, &evaluation_context).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;

        Self::from_workflow(workflow_source, workflow, mcp_pool)
    }

    fn from_workflow(workflow_source: &str, workflow: Workflow, mcp_pool: McpClientPool) -> Result<Self, ExecutorError> {
        log::debug!("validating workflow after schema discovery");
        let validation_report = validate_workflow(&workflow);

        if validation_report.has_issues() {
            let issues = validation_report.render_with_source(workflow_source, "<workflow>");

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

    #[must_use]
    pub fn agent_execution_order(&self) -> Vec<String> {
        self.execution_plan.agent_execution_order.clone()
    }

    #[must_use]
    pub fn mcp_imports(&self) -> &[superwire_core::semantic::PlannedMcpImport] {
        &self.execution_plan.mcp_imports
    }

    pub async fn execute<ModelProviderType>(
        &self,
        input: Value,
        secrets: Value,
        model_provider: &ModelProviderType,
        event_sender: Option<mpsc::Sender<ExecutorEvent>>,
        max_concurrency: usize,
    ) -> Result<Value, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let input_values = self.resolve_input_values(&input)?;
        let secret_values = self.resolve_secret_values(&secrets)?;
        let mut runtime_state = RuntimeState::new(input_values, secret_values);
        let tool_call_tracker = ToolCallTracker::default();

        log::info!("executing workflow runtime");

        self.execute_workflow_dynamic_blocks(&mut runtime_state, event_sender.as_ref(), &tool_call_tracker)?;

        let import_context = self.resolve_mcp_import_context(&runtime_state.evaluation_context(HashMap::new()))?;

        log::debug!(
            "workflow-level import context resolved: {}",
            if import_context.is_empty() { "empty" } else { "populated" }
        );

        for execution_batch in self.resolve_agent_execution_batches()? {
            let runtime_state_snapshot = runtime_state.clone();
            let mut for_loop_agents = Vec::new();
            let mut regular_agents = Vec::new();

            log::debug!("starting execution batch: agents={execution_batch:?}");

            for agent_name in execution_batch {
                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(&agent_name)
                    .expect("planned agent should exist")
                    .clone();

                if planned_agent.declaration.for_loop.is_some() {
                    for_loop_agents.push(planned_agent);
                } else {
                    regular_agents.push(planned_agent);
                }
            }

            let agent_execution_context = AgentExecutionContext {
                event_sender: event_sender.clone(),
                import_context: import_context.clone(),
                tool_call_tracker: tool_call_tracker.clone(),
            };

            for planned_agent in for_loop_agents {
                let completed = self
                    .execute_for_loop_agent(
                        planned_agent,
                        &runtime_state_snapshot,
                        model_provider,
                        max_concurrency,
                        &agent_execution_context,
                    )
                    .await?;
                completed.apply_to_runtime_state(&mut runtime_state);
            }

            let mut pending_executions = Vec::new();

            for planned_agent in regular_agents {
                let runtime_state_snapshot = runtime_state_snapshot.clone();
                let agent_execution_context = agent_execution_context.clone();

                pending_executions.push(async move {
                    self.execute_agent(&planned_agent, &runtime_state_snapshot, model_provider, &agent_execution_context)
                        .await
                });
            }

            let completed_executions = try_join_all(pending_executions).await?;

            for completed_execution in completed_executions {
                completed_execution.apply_to_runtime_state(&mut runtime_state);
            }
        }

        let output = self.evaluate_workflow_output(&runtime_state, &tool_call_tracker)?;

        validate_value_against_type(&output, &self.execution_plan.workflow_output_type).map_err(|message| {
            ExecutorError::OutputTypeMismatch {
                expected: self.execution_plan.workflow_output_type.to_string(),
                found: format!("invalid runtime output: {message}"),
            }
        })?;

        log::info!("workflow runtime completed");

        Ok(output)
    }

    fn resolve_input_values(&self, input: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(input_type) = &self.execution_plan.input_type {
            if input.is_null() {
                if let WorkflowType::Object(field_types) = input_type {
                    let tool_consumed_fields = self.input_fields_consumed_by_bindings();

                    if field_types.keys().all(|field_name| tool_consumed_fields.contains(field_name)) {
                        let input_map = field_types
                            .keys()
                            .map(|field_name| (field_name.clone(), Value::Null))
                            .collect::<Map<String, Value>>();

                        return Ok(input_map);
                    }

                    let uncovered_fields = field_types
                        .keys()
                        .filter(|field_name| !tool_consumed_fields.contains(field_name.as_str()))
                        .cloned()
                        .collect::<Vec<_>>();

                    return Err(ExecutorError::InputValueMismatch {
                        message: format!(
                            "workflow declares an `input` block, but no input object was provided; \
                             the following fields are not covered by tool bindings and must be provided: {}",
                            uncovered_fields.join(", ")
                        ),
                    });
                }

                return Err(ExecutorError::InputValueMismatch {
                    message: format!("workflow declares an `input` block, but no input object was provided; expected {input_type}"),
                });
            }

            validate_value_against_type(input, input_type).map_err(|message| ExecutorError::InputValueMismatch {
                message: format!("declared `input` block expects {input_type}: {message}"),
            })?;

            return input.as_object().cloned().ok_or_else(|| ExecutorError::InputValueMismatch {
                message: format!(
                    "declared `input` block expects object matching {input_type}, found {}",
                    value_kind_name(input)
                ),
            });
        }

        if input.is_null() || input.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: value_kind_name(input).to_string(),
        })
    }

    fn input_fields_consumed_by_bindings(&self) -> HashSet<String> {
        let mut consumed_fields = HashSet::new();

        for tool in self.execution_plan.tools.values() {
            for fixed_binding in &tool.declaration.fixed_binding_fields {
                if let Expression::Reference(reference) = &fixed_binding.value {
                    if reference.root_keyword() == Some(ReferenceKeyword::Input) {
                        if let Some(field_name) = reference.first_access_field() {
                            consumed_fields.insert(field_name.to_string());
                        }
                    }
                }
            }
        }

        consumed_fields
    }

    fn resolve_secret_values(&self, secrets: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(secrets_type) = &self.execution_plan.secrets_type {
            if secrets.is_null() {
                return Err(ExecutorError::SecretValueMismatch {
                    message: format!("workflow declares a `secrets` block, but no secrets object was provided; expected {secrets_type}"),
                });
            }

            validate_value_against_type(secrets, secrets_type).map_err(|message| ExecutorError::SecretValueMismatch {
                message: format!("declared `secrets` block expects {secrets_type}: {message}"),
            })?;

            return secrets.as_object().cloned().ok_or_else(|| ExecutorError::SecretValueMismatch {
                message: format!(
                    "declared `secrets` block expects object matching {secrets_type}, found {}",
                    value_kind_name(secrets)
                ),
            });
        }

        if secrets.is_null() || secrets.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no secrets".to_string(),
            found: value_kind_name(secrets).to_string(),
        })
    }

    fn resolve_agent_execution_batches(&self) -> Result<Vec<Vec<String>>, ExecutorError> {
        let execution_order = &self.execution_plan.agent_execution_order;
        let mut unresolved_agents = execution_order.iter().cloned().collect::<HashSet<_>>();
        let mut resolved_agents = HashSet::<String>::new();
        let mut execution_batches = Vec::<Vec<String>>::new();

        while !unresolved_agents.is_empty() {
            let mut ready_agents = Vec::<String>::new();

            for agent_name in execution_order {
                if !unresolved_agents.contains(agent_name) {
                    continue;
                }

                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(agent_name)
                    .expect("planned agent should exist");

                if planned_agent
                    .dependencies
                    .iter()
                    .any(|dependency_name| !resolved_agents.contains(dependency_name))
                {
                    continue;
                }

                ready_agents.push(agent_name.clone());
            }

            if ready_agents.is_empty() {
                return Err(ExecutorError::Other {
                    message: "failed to resolve execution batches".to_string(),
                });
            }

            for ready_agent_name in &ready_agents {
                unresolved_agents.remove(ready_agent_name);
                resolved_agents.insert(ready_agent_name.clone());
            }

            execution_batches.push(ready_agents);
        }

        Ok(execution_batches)
    }

    fn execute_workflow_dynamic_blocks(
        &self,
        runtime_state: &mut RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<(), ExecutorError> {
        for declaration in self.workflow.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    &runtime_state.evaluation_context(HashMap::new()),
                    &format!("dynamic field `{}`", dynamic_field.name),
                    event_sender,
                    tool_call_tracker,
                )?;
                runtime_state.insert_local_binding(dynamic_field.name.clone(), field_value);
            }
        }

        Ok(())
    }

    fn execute_deterministic_tool_call(
        &self,
        tool_call: &ToolCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<Value, ExecutorError> {
        let tool_name = tool_call.callee.tool_name().ok_or_else(|| ExecutorError::Other {
            message: "deterministic tool call must use `tool.<name>` reference".to_string(),
        })?;
        log::debug!("executing deterministic tool call `{tool_name}`");
        let typed_tool = self.execution_plan.tools.get(tool_name).ok_or_else(|| ExecutorError::Other {
            message: format!("deterministic tool call references unknown tool `{tool_name}`"),
        })?;

        tool_call_tracker
            .register_call(tool_name, typed_tool.declaration.max_calls, &ToolCallLimitScope::Workflow)
            .map_err(|message| ExecutorError::Other { message })?;

        let bindings = typed_tool.resolve_bindings(&tool_call.binding_fields, evaluation_context)?;
        let source = self.model_tool_source(&typed_tool.declaration, evaluation_context)?;
        let mut input_arguments = Map::new();

        for input_field in &tool_call.input_fields {
            let input_value = self.evaluate_runtime_expression(
                &input_field.value,
                evaluation_context,
                &format!("input field `{}` for tool `{}`", input_field.name, tool_name),
                event_sender,
                tool_call_tracker,
            )?;
            input_arguments.insert(input_field.name.clone(), input_value);
        }

        validate_value_against_type(&Value::Object(input_arguments.clone()), &typed_tool.input_type).map_err(|message| {
            ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` input is invalid: {message}"),
            }
        })?;

        let mut arguments = input_arguments;

        if let Some(binding_object) = bindings.as_object() {
            for (binding_name, binding_value) in binding_object {
                arguments.insert(binding_name.clone(), binding_value.clone());
            }
        }

        match source {
            ModelToolSource::Mcp {
                server_name,
                tool_name: mcp_tool_name,
                endpoint,
                headers,
            } => {
                let server_config = McpServerConfig {
                    name: server_name.unwrap_or_else(|| "default".to_string()),
                    endpoint,
                    headers,
                };

                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::tool_call_started(
                        String::new(),
                        tool_name.to_string(),
                        Value::Object(arguments.clone()),
                    ));
                }

                log::info!("calling MCP tool `{mcp_tool_name}` for deterministic tool `{tool_name}`");
                let result = self
                    .mcp_pool
                    .get(&server_config)?
                    .call_tool(&mcp_tool_name, Value::Object(arguments))
                    .map_err(|error| ExecutorError::Other {
                        message: format!("deterministic tool call `{tool_name}` failed: {error}"),
                    })?;
                let normalized_result = normalize_mcp_tool_result(result);

                log::debug!("completed deterministic MCP tool `{tool_name}`");

                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::tool_call_completed(
                        String::new(),
                        tool_name.to_string(),
                        normalized_result.clone(),
                    ));
                }

                Ok(normalized_result)
            }
            ModelToolSource::Local => Err(ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` is not backed by MCP"),
            }),
        }
    }

    async fn execute_agent<ModelProviderType>(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        model_provider: &ModelProviderType,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let agent_dynamic_values = self.execute_agent_dynamic_blocks(
            planned_agent,
            runtime_state,
            agent_execution_context.event_sender.as_ref(),
            &agent_execution_context.tool_call_tracker,
        )?;
        let evaluation_context = runtime_state.evaluation_context(agent_dynamic_values);
        log::info!("starting agent `{}`", planned_agent.name);
        let provider_template = self
            .execution_plan
            .provider_index
            .get(&planned_agent.provider_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("provider `{}` is not declared", planned_agent.provider_name),
            })?;
        let provider_config = provider_template.resolve(&planned_agent.provider_name, &evaluation_context)?;
        let ProviderConfig::OpenAI(openai_provider_config) = provider_config else {
            return Err(ExecutorError::Other {
                message: "executor only supports provider driver `openai`".to_string(),
            });
        };
        let model_name = evaluate_agent_model_name(&planned_agent.model_expression, &planned_agent.name, &evaluation_context)?;
        let prompt_expression = planned_agent
            .declaration
            .required_expression_property(AgentExpressionPropertyName::Prompt)
            .map_err(|missing_property| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: planned_agent.name.clone(),
                property: missing_property.as_str().to_string(),
                message: "property is required".to_string(),
            })?;
        let agent_prompt = normalize_prompt(self.evaluate_runtime_expression(
            prompt_expression,
            &evaluation_context,
            &format!("prompt for agent `{}`", planned_agent.name),
            None,
            &agent_execution_context.tool_call_tracker,
        )?);
        let prompt = if agent_execution_context.import_context.is_empty() {
            agent_prompt
        } else {
            format!("{}\n\n{agent_prompt}", agent_execution_context.import_context)
        };
        let output_schema = workflow_type_to_json_schema(&planned_agent.iteration_output_type);
        let tool_definitions = self.resolve_agent_tool_definitions(planned_agent, &evaluation_context)?;
        let tool_names = tool_definitions
            .iter()
            .map(|tool_definition| tool_definition.name.clone())
            .collect::<Vec<_>>();

        log::debug!(
            "agent `{}` request prepared: model={}, tools={}, response_schema={}",
            planned_agent.name,
            model_name,
            tool_definitions.len(),
            output_schema.get("type").and_then(Value::as_str).unwrap_or("unknown")
        );

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_started(
                    planned_agent.name.clone(),
                    model_name.clone(),
                    tool_names,
                ))
                .await;
        }

        let model_response = model_provider
            .generate(ModelRequest {
                agent_name: planned_agent.name.clone(),
                provider_config: openai_provider_config,
                model_name,
                prompt,
                output_schema,
                response_format: Self::agent_response_format(planned_agent),
                tools: tool_definitions,
                event_sender: agent_execution_context.event_sender.clone(),
                mcp_pool: self.mcp_pool.clone(),
                tool_call_tracker: agent_execution_context.tool_call_tracker.clone(),
            })
            .await?;

        log::debug!("agent `{}` model response received", planned_agent.name);

        validate_agent_output_value(&model_response.output, &planned_agent.iteration_output_type, &planned_agent.name)?;

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_completed(
                    planned_agent.name.clone(),
                    model_response.output.clone(),
                ))
                .await;
        }

        Ok(CompletedAgentExecution {
            agent_name: planned_agent.name.clone(),
            output: model_response.output,
            context: model_response.context,
        })
    }

    async fn execute_for_loop_agent<ModelProviderType>(
        &self,
        planned_agent: PlannedAgent,
        runtime_state: &RuntimeState,
        model_provider: &ModelProviderType,
        max_concurrency: usize,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let for_loop = planned_agent
            .declaration
            .for_loop
            .as_ref()
            .expect("for-loop agent must have for_loop");
        let loop_pattern = for_loop.pattern.clone();
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());
        let iterable_value = evaluate_expression(
            &for_loop.iterable,
            &evaluation_context,
            &format!("for-loop iterable for agent `{}`", planned_agent.name),
        )?;
        let items = iterable_value.as_array().ok_or_else(|| ExecutorError::Other {
            message: format!(
                "for-loop iterable for agent `{}` must evaluate to an array, found {}",
                planned_agent.name,
                value_kind_name(&iterable_value)
            ),
        })?;

        if items.is_empty() {
            return Ok(CompletedAgentExecution {
                agent_name: planned_agent.name.clone(),
                output: Value::Array(Vec::new()),
                context: Value::Null,
            });
        }

        let concurrency_limit = max_concurrency.max(1);
        let semaphore = Arc::new(Semaphore::new(concurrency_limit));
        let mut pending_iterations = FuturesUnordered::new();
        let agent_name = planned_agent.name.clone();
        let tool_call_tracker = runtime_state.tool_call_tracker();

        for item in items {
            let mut iteration_state = runtime_state.clone();
            Self::bind_loop_variables(&loop_pattern, item, &mut iteration_state)?;
            let semaphore_clone = semaphore.clone();
            let agent_clone = planned_agent.clone();
            let iteration_execution_context = AgentExecutionContext {
                event_sender: agent_execution_context.event_sender.clone(),
                import_context: agent_execution_context.import_context.clone(),
                tool_call_tracker: tool_call_tracker.clone(),
            };

            pending_iterations.push(async move {
                let permit = semaphore_clone.acquire_owned().await.map_err(|error| ExecutorError::Other {
                    message: format!("failed to acquire concurrency permit: {error}"),
                })?;
                let result = self
                    .execute_agent(&agent_clone, &iteration_state, model_provider, &iteration_execution_context)
                    .await;
                drop(permit);
                result
            });
        }

        let mut iteration_outputs = Vec::with_capacity(pending_iterations.len());

        while let Some(iteration_result) = pending_iterations.next().await {
            iteration_outputs.push(iteration_result?.output);
        }

        Ok(CompletedAgentExecution {
            agent_name,
            output: Value::Array(iteration_outputs),
            context: Value::Null,
        })
    }

    fn bind_loop_variables(pattern: &AgentForLoopPattern, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        match pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                runtime_state.insert_local_binding(identifier.clone(), item.clone());
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                let item_object = item.as_object().ok_or_else(|| ExecutorError::Other {
                    message: format!("for-loop destructuring expects object, found {}", value_kind_name(item)),
                })?;

                for field_name in field_names {
                    let field_value = item_object.get(field_name).cloned().unwrap_or(Value::Null);
                    runtime_state.insert_local_binding(field_name.clone(), field_value);
                }
            }
        }

        Ok(())
    }

    fn evaluate_workflow_output(&self, runtime_state: &RuntimeState, tool_call_tracker: &ToolCallTracker) -> Result<Value, ExecutorError> {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());

        for output_field in &self.execution_plan.output_declaration.fields {
            let output_value =
                self.evaluate_runtime_expression(&output_field.value, &evaluation_context, "workflow output", None, tool_call_tracker)?;
            output_fields.insert(output_field.name.clone(), output_value);
        }

        Ok(Value::Object(output_fields))
    }

    fn execute_agent_dynamic_blocks(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<HashMap<String, Value>, ExecutorError> {
        let mut dynamic_values = HashMap::new();

        for agent_property in &planned_agent.declaration.properties {
            let AgentProperty::Dynamic(dynamic_block) = agent_property else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    &runtime_state.evaluation_context(dynamic_values.clone()),
                    &format!("dynamic field `{}` for agent `{}`", dynamic_field.name, planned_agent.name),
                    event_sender,
                    tool_call_tracker,
                )?;
                dynamic_values.insert(dynamic_field.name.clone(), field_value);
            }
        }

        Ok(dynamic_values)
    }

    fn resolve_agent_tool_definitions(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<Vec<ModelToolDefinition>, ExecutorError> {
        let Some(tools_expression) = planned_agent.declaration.expression_property(AgentExpressionPropertyName::Tools) else {
            return Ok(Vec::new());
        };
        let Expression::ArrayLiteral(tool_expressions) = tools_expression else {
            return Err(ExecutorError::Other {
                message: format!("tools for agent `{}` must be an array", planned_agent.name),
            });
        };
        let mut tool_definitions = Vec::new();

        for tool_expression in tool_expressions {
            tool_definitions.push(self.resolve_agent_tool_definition(tool_expression, planned_agent, evaluation_context)?);
        }

        Ok(tool_definitions)
    }

    fn evaluate_runtime_expression(
        &self,
        expression: &Expression,
        evaluation_context: &EvaluationContext,
        context: &str,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<Value, ExecutorError> {
        match expression {
            Expression::StringLiteral(string_literal) => Ok(Value::String(string_literal.clone())),
            Expression::StringTemplate(string_template) => {
                let mut rendered_template = String::new();

                for string_template_part in &string_template.parts {
                    match string_template_part {
                        superwire_core::dsl::StringTemplatePart::Text(template_text) => rendered_template.push_str(template_text),
                        superwire_core::dsl::StringTemplatePart::Interpolation(interpolation_expression) => {
                            let interpolation_value = self.evaluate_runtime_expression(
                                interpolation_expression,
                                evaluation_context,
                                context,
                                event_sender,
                                tool_call_tracker,
                            )?;
                            rendered_template.push_str(&normalize_prompt(interpolation_value));
                        }
                    }
                }

                Ok(Value::String(rendered_template))
            }
            Expression::NumberLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral
            | Expression::Reference(_)
            | Expression::FunctionCall(_) => Ok(evaluate_expression(expression, evaluation_context, context)?),
            Expression::ToolCall(tool_call) => {
                self.execute_deterministic_tool_call(tool_call, evaluation_context, event_sender, tool_call_tracker)
            }
            Expression::McpCall(mcp_call) => self.execute_mcp_call(mcp_call, evaluation_context, event_sender),
            Expression::ArrayLiteral(array_items) => {
                let mut evaluated_items = Vec::with_capacity(array_items.len());

                for array_item in array_items {
                    evaluated_items.push(self.evaluate_runtime_expression(
                        array_item,
                        evaluation_context,
                        context,
                        event_sender,
                        tool_call_tracker,
                    )?);
                }

                Ok(Value::Array(evaluated_items))
            }
            Expression::ObjectLiteral(object_fields) => {
                let mut evaluated_fields = Map::new();

                for object_field in object_fields {
                    let field_value = self.evaluate_runtime_expression(
                        &object_field.value,
                        evaluation_context,
                        context,
                        event_sender,
                        tool_call_tracker,
                    )?;
                    evaluated_fields.insert(object_field.name.clone(), field_value);
                }

                Ok(Value::Object(evaluated_fields))
            }
        }
    }

    fn execute_mcp_call(
        &self,
        mcp_call: &superwire_core::dsl::McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let target_name = mcp_call.target_name().ok_or_else(|| ExecutorError::Other {
            message: format!("{} call requires a target name", mcp_call.operation.as_str()),
        })?;
        let expected_root = mcp_call.operation.expected_root();

        if mcp_call.callee.root_keyword() != Some(expected_root) {
            return Err(ExecutorError::Other {
                message: format!(
                    "{} call must target `{}.<name>`",
                    mcp_call.operation.as_str(),
                    expected_root.as_str()
                ),
            });
        }

        match mcp_call.operation {
            superwire_core::dsl::McpCallOperation::Read => {
                self.execute_resource_read(target_name, mcp_call, evaluation_context, event_sender)
            }
            superwire_core::dsl::McpCallOperation::Render => {
                self.execute_prompt_render(target_name, mcp_call, evaluation_context, event_sender)
            }
        }
    }

    fn execute_resource_read(
        &self,
        resource_name: &str,
        mcp_call: &superwire_core::dsl::McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let resource_import = self
            .workflow
            .find_resource_import(resource_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("resource `{resource_name}` is not imported"),
            })?;
        let server_config = self.resolve_mcp_import_server(&resource_import.source.server_name, evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &resource_import.parameters,
            &mcp_call.parameter_fields,
            evaluation_context,
            resource_name,
        )?;
        let operation = mcp_call.operation.as_str().to_string();

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_started(
                operation.clone(),
                resource_name.to_string(),
                arguments.clone(),
            ));
        }

        let result = match self
            .mcp_pool
            .get(&server_config)?
            .read_resource(&resource_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_failed(
                        operation,
                        resource_name.to_string(),
                        Value::String(error.to_string()),
                    ));
                }

                return Err(ExecutorError::Other {
                    message: format!("MCP resource `{resource_name}` failed: {error}"),
                });
            }
        };
        let rendered_result = Value::String(render_mcp_resource_result(&result));

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_completed(
                mcp_call.operation.as_str().to_string(),
                resource_name.to_string(),
                rendered_result.clone(),
            ));
        }

        Ok(rendered_result)
    }

    fn execute_prompt_render(
        &self,
        prompt_name: &str,
        mcp_call: &superwire_core::dsl::McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let prompt_import = self.workflow.find_prompt_import(prompt_name).ok_or_else(|| ExecutorError::Other {
            message: format!("prompt `{prompt_name}` is not imported"),
        })?;
        let server_config = self.resolve_mcp_import_server(&prompt_import.source.server_name, evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &prompt_import.parameters,
            &mcp_call.parameter_fields,
            evaluation_context,
            prompt_name,
        )?;
        let operation = mcp_call.operation.as_str().to_string();

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_started(
                operation.clone(),
                prompt_name.to_string(),
                arguments.clone(),
            ));
        }

        let result = match self
            .mcp_pool
            .get(&server_config)?
            .get_prompt(&prompt_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_failed(
                        operation,
                        prompt_name.to_string(),
                        Value::String(error.to_string()),
                    ));
                }

                return Err(ExecutorError::Other {
                    message: format!("MCP prompt `{prompt_name}` failed: {error}"),
                });
            }
        };
        let rendered_result = Value::String(render_mcp_prompt_result(&result));

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_completed(
                mcp_call.operation.as_str().to_string(),
                prompt_name.to_string(),
                rendered_result.clone(),
            ));
        }

        Ok(rendered_result)
    }

    fn resolve_mcp_call_parameters(
        &self,
        import_parameters: &[ObjectField],
        call_parameters: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let mut resolved_parameters = Map::new();

        for parameter in import_parameters.iter().chain(call_parameters.iter()) {
            let parameter_value = self.evaluate_runtime_expression(
                &parameter.value,
                evaluation_context,
                &format!("MCP call `{import_name}` parameter `{}`", parameter.name),
                None,
                &ToolCallTracker::default(),
            )?;
            resolved_parameters.insert(parameter.name.clone(), parameter_value);
        }

        Ok(Value::Object(resolved_parameters))
    }

    fn resolve_mcp_import_context(&self, evaluation_context: &EvaluationContext) -> Result<String, ExecutorError> {
        let mut context_sections = Vec::new();

        for prompt_import in self.workflow.prompt_imports() {
            let server_config = self.resolve_mcp_import_server(&prompt_import.source.server_name, evaluation_context)?;
            let arguments = self.resolve_mcp_import_parameters(&prompt_import.parameters, evaluation_context, &prompt_import.name)?;
            let result = self
                .mcp_pool
                .get(&server_config)?
                .get_prompt(&prompt_import.source.item_name, arguments)
                .map_err(|error| ExecutorError::Other {
                    message: format!("MCP prompt `{}` failed: {error}", prompt_import.name),
                })?;

            context_sections.push(format!(
                "MCP prompt `{}`:\n{}",
                prompt_import.name,
                render_mcp_prompt_result(&result)
            ));
        }

        for resource_import in self.workflow.resource_imports() {
            let server_config = self.resolve_mcp_import_server(&resource_import.source.server_name, evaluation_context)?;
            let arguments = self.resolve_mcp_import_parameters(&resource_import.parameters, evaluation_context, &resource_import.name)?;
            let result = self
                .mcp_pool
                .get(&server_config)?
                .read_resource(&resource_import.source.item_name, arguments)
                .map_err(|error| ExecutorError::Other {
                    message: format!("MCP resource `{}` failed: {error}", resource_import.name),
                })?;

            context_sections.push(format!(
                "MCP resource `{}`:\n{}",
                resource_import.name,
                render_mcp_resource_result(&result)
            ));
        }

        Ok(context_sections.join("\n\n"))
    }

    fn resolve_mcp_import_server(
        &self,
        server_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<McpServerConfig, ExecutorError> {
        let mcp_server_declaration = self.workflow.find_mcp_server(server_name).ok_or_else(|| ExecutorError::Other {
            message: format!("MCP import references unknown MCP server `{server_name}`"),
        })?;

        McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })
    }

    fn resolve_mcp_import_parameters(
        &self,
        parameters: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let mut resolved_parameters = Map::new();

        for parameter in parameters {
            let parameter_value = self.evaluate_runtime_expression(
                &parameter.value,
                evaluation_context,
                &format!("MCP import `{import_name}` parameter `{}`", parameter.name),
                None,
                &ToolCallTracker::default(),
            )?;
            resolved_parameters.insert(parameter.name.clone(), parameter_value);
        }

        Ok(Value::Object(resolved_parameters))
    }

    fn resolve_agent_tool_definition(
        &self,
        tool_expression: &Expression,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<ModelToolDefinition, ExecutorError> {
        let (tool_reference, override_binding_fields, override_max_calls) = match tool_expression {
            Expression::Reference(reference) => (reference, Vec::new(), None),
            Expression::ToolCall(tool_call) => (&tool_call.callee, tool_call.binding_fields.clone(), tool_call.max_calls),
            _ => {
                return Err(ExecutorError::Other {
                    message: format!("tools for agent `{}` must contain tool references", planned_agent.name),
                });
            }
        };
        let tool_name = tool_reference.tool_name().ok_or_else(|| ExecutorError::Other {
            message: format!("tools for agent `{}` must use `tool.<name>` references", planned_agent.name),
        })?;
        let typed_tool = self.execution_plan.tools.get(tool_name).ok_or_else(|| ExecutorError::Other {
            message: format!("agent `{}` references unknown tool `{tool_name}`", planned_agent.name),
        })?;
        let bindings = typed_tool.resolve_bindings(&override_binding_fields, evaluation_context)?;
        log::debug!(
            "resolved tool `{}` for agent `{}`: binding_keys={}",
            typed_tool.name,
            planned_agent.name,
            bindings.as_object().map_or(0, serde_json::Map::len)
        );

        Ok(ModelToolDefinition {
            name: typed_tool.name.clone(),
            description: typed_tool.declaration.description.clone(),
            source: self.model_tool_source(&typed_tool.declaration, evaluation_context)?,
            input_schema: typed_tool.model_input_schema(&bindings),
            output_schema: workflow_type_to_json_schema(&typed_tool.output_type),
            bindings,
            max_calls: override_max_calls.or(typed_tool.declaration.max_calls),
            max_calls_scope: if override_max_calls.is_some() {
                ToolCallLimitScope::Agent {
                    agent_name: planned_agent.name.clone(),
                }
            } else {
                ToolCallLimitScope::Workflow
            },
        })
    }

    fn model_tool_source(
        &self,
        tool_declaration: &superwire_core::dsl::ToolDeclaration,
        evaluation_context: &EvaluationContext,
    ) -> Result<ModelToolSource, ExecutorError> {
        let Some(ToolSource::Mcp(mcp_tool_source)) = &tool_declaration.source else {
            return Ok(ModelToolSource::Local);
        };
        let is_server_only_source =
            mcp_tool_source.server_name.is_none() && self.workflow.find_mcp_server(&mcp_tool_source.tool_name).is_some();
        let resolved_server_name = if is_server_only_source {
            Some(mcp_tool_source.tool_name.as_str())
        } else {
            mcp_tool_source.server_name.as_deref()
        };
        let mcp_server_declaration = if let Some(server_name) = resolved_server_name {
            self.workflow.find_mcp_server(server_name).ok_or_else(|| ExecutorError::Other {
                message: format!("tool `{}` references unknown MCP server `{server_name}`", tool_declaration.name),
            })?
        } else {
            self.workflow
                .declarations()
                .iter()
                .find_map(|declaration| match declaration {
                    Declaration::McpServer(mcp_server_declaration) => Some(mcp_server_declaration),
                    _ => None,
                })
                .ok_or_else(|| ExecutorError::Other {
                    message: format!("tool `{}` uses MCP but no `mcp` server is declared", tool_declaration.name),
                })?
        };
        let mcp_server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context).map_err(|error| {
            ExecutorError::Other {
                message: error.to_string(),
            }
        })?;

        Ok(ModelToolSource::Mcp {
            server_name: resolved_server_name.map(str::to_string),
            tool_name: if is_server_only_source {
                tool_declaration.name.clone()
            } else {
                mcp_tool_source.tool_name.clone()
            },
            endpoint: mcp_server_config.endpoint,
            headers: mcp_server_config.headers,
        })
    }
}

trait ToolReferenceExt {
    fn tool_name(&self) -> Option<&str>;
}

impl ToolReferenceExt for Reference {
    fn tool_name(&self) -> Option<&str> {
        if self.root_keyword() != Some(ReferenceKeyword::Tool) {
            return None;
        }

        self.first_access_field()
    }
}

trait TypedToolModelSchemaExt {
    fn model_input_schema(&self, bindings: &Value) -> Value;
}

impl TypedToolModelSchemaExt for TypedToolIr {
    fn model_input_schema(&self, bindings: &Value) -> Value {
        let mut input_schema = workflow_type_to_json_schema(&self.input_type);
        let Some(binding_object) = bindings.as_object() else {
            return input_schema;
        };
        let binding_names = binding_object.keys().cloned().collect::<HashSet<_>>();

        if let Some(properties) = input_schema.get_mut("properties").and_then(Value::as_object_mut) {
            for binding_name in &binding_names {
                properties.remove(binding_name);
            }
        }

        let mut remove_required = false;

        if let Some(required_fields) = input_schema.get_mut("required").and_then(Value::as_array_mut) {
            required_fields.retain(|required_field| {
                required_field
                    .as_str()
                    .is_none_or(|required_field_name| !binding_names.contains(required_field_name))
            });
            remove_required = required_fields.is_empty();
        }

        if remove_required {
            if let Some(schema_object) = input_schema.as_object_mut() {
                schema_object.remove("required");
            }
        }

        input_schema
    }
}

trait TypedToolRuntimeExt {
    fn resolve_bindings(
        &self,
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
    ) -> Result<Value, ExecutorError>;
}

impl TypedToolRuntimeExt for TypedToolIr {
    fn resolve_bindings(
        &self,
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
    ) -> Result<Value, ExecutorError> {
        let mut binding_values = Map::new();

        for fixed_binding_field in &self.declaration.fixed_binding_fields {
            let binding_value = evaluate_expression(
                &fixed_binding_field.value,
                evaluation_context,
                &format!("fixed binding `{}` for tool `{}`", fixed_binding_field.name, self.name),
            )?;
            binding_values.insert(fixed_binding_field.name.clone(), binding_value);
        }

        let mut typed_binding_values = Map::new();

        for override_binding_field in override_binding_fields {
            let binding_value = evaluate_expression(
                &override_binding_field.value,
                evaluation_context,
                &format!("binding `{}` for tool `{}`", override_binding_field.name, self.name),
            )?;
            binding_values.insert(override_binding_field.name.clone(), binding_value.clone());
            typed_binding_values.insert(override_binding_field.name.clone(), binding_value);
        }

        validate_value_against_type(&Value::Object(typed_binding_values), &self.binding_type).map_err(|message| ExecutorError::Other {
            message: format!("tool `{}` binding values are invalid: {message}", self.name),
        })?;

        Ok(Value::Object(binding_values))
    }
}

fn evaluate_agent_model_name(
    model_expression: &Expression,
    agent_name: &str,
    evaluation_context: &EvaluationContext,
) -> Result<String, ExecutorError> {
    let model_value = evaluate_expression(model_expression, evaluation_context, &format!("model for agent `{agent_name}`"))?;

    model_value.as_str().map(str::to_string).ok_or_else(|| ExecutorError::Other {
        message: format!("model for agent `{agent_name}` must resolve to string"),
    })
}

fn normalize_prompt(prompt_value: Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    serde_json::to_string(&prompt_value).unwrap_or_else(|_| prompt_value.to_string())
}

fn render_mcp_prompt_result(result: &Value) -> String {
    let Some(messages) = result.get("messages").and_then(Value::as_array) else {
        return normalize_prompt(result.clone());
    };
    let mut rendered_messages = Vec::new();

    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("message");
        let content = message.get("content").map_or_else(String::new, render_mcp_content_value);
        rendered_messages.push(format!("{role}: {content}"));
    }

    rendered_messages.join("\n")
}

fn render_mcp_resource_result(result: &Value) -> String {
    let Some(contents) = result.get("contents").and_then(Value::as_array) else {
        return normalize_prompt(result.clone());
    };
    let mut rendered_contents = Vec::new();

    for content in contents {
        rendered_contents.push(render_mcp_content_value(content));
    }

    rendered_contents.join("\n")
}

fn render_mcp_content_value(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(blob) = content.get("blob").and_then(Value::as_str) {
        return blob.to_string();
    }

    normalize_prompt(content.clone())
}

fn value_object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn validate_agent_output_value(output: &Value, expected_type: &WorkflowType, agent_name: &str) -> Result<(), ExecutorError> {
    validate_value_against_type(output, expected_type).map_err(|message| ExecutorError::AgentOutputTypeMismatch {
        agent_name: agent_name.to_string(),
        message,
    })
}
