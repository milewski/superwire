use crate::ast::Workflow;
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::execution::orchestrator::AgentOrchestrator;
use crate::parser::{AstBuilder, DependencyGraph};
use crate::providers::{ProviderFactory, ProviderRef, ProviderRegistry};
use crate::tools::ToolRegistry;
use crate::validation::WorkflowValidator;
use serde_json::Value;
use std::collections::HashMap;

type AgentTaskHandle =
    tokio::task::JoinHandle<Result<(String, Value, Vec<crate::providers::provider::Message>), ExecutionError>>;

/// Handler for compact function execution
struct CompactFunctionHandler<'a> {
    provider_registry: &'a ProviderRegistry,
}

impl<'a> CompactFunctionHandler<'a> {
    fn new(provider_registry: &'a ProviderRegistry) -> Self {
        Self { provider_registry }
    }

    async fn execute(
        &self,
        function_call: &crate::ast::FunctionCall,
        runtime_context: &RuntimeContext,
    ) -> Result<Value, ExecutionError> {
        let model_ref = self.extract_model_reference(function_call)?;
        let provider = self.get_provider_for_model(&model_ref)?;
        let combined_messages = self.extract_and_combine_messages(function_call, runtime_context)?;

        self.execute_compact_operation(provider, combined_messages, &model_ref)
            .await
    }

    fn extract_model_reference(&self, function_call: &crate::ast::FunctionCall) -> Result<String, ExecutionError> {
        let model_value = function_call
            .arguments
            .get("model")
            .ok_or_else(|| ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "compact function requires 'model' argument".to_string(),
                suggestion: Some("Provide a model like 'ollama1/qwen3:8b'".to_string()),
            })?;

        match model_value {
            crate::ast::Value::String(string) => Ok(string.clone()),
            crate::ast::Value::Interpolated(string) => Ok(string.clone()),
            _ => Err(ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "model must be a string".to_string(),
                suggestion: None,
            }),
        }
    }

    fn get_provider_for_model(&self, model_ref: &str) -> Result<ProviderRef, ExecutionError> {
        let (provider, _model) =
            self.provider_registry
                .get_model_provider(model_ref)
                .map_err(|error| ExecutionError::ProviderError {
                    agent: "compact".to_string(),
                    message: error.to_string(),
                    suggestion: Some("Check provider and model configuration".to_string()),
                })?;

        Ok(provider)
    }

    fn extract_and_combine_messages(
        &self,
        function_call: &crate::ast::FunctionCall,
        runtime_context: &RuntimeContext,
    ) -> Result<Vec<crate::providers::provider::Message>, ExecutionError> {
        let context_value = function_call
            .arguments
            .get("context")
            .ok_or_else(|| ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "compact function requires 'context' argument".to_string(),
                suggestion: Some("Provide agent context like 'agent.name.context'".to_string()),
            })?;

        let resolved_context = runtime_context.resolve_value(context_value)?;
        let mut combined_messages = Vec::new();

        if let Value::Array(items) = &resolved_context {
            if items.is_empty() {
                return Err(ExecutionError::RuntimeError {
                    agent: "compact".to_string(),
                    message: "No messages found in context to compact".to_string(),
                    suggestion: Some("Ensure the context reference points to a valid agent context".to_string()),
                });
            }

            self.process_context_items(items, &mut combined_messages);
        }

        if combined_messages.is_empty() {
            return Err(ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "No messages found in context to compact".to_string(),
                suggestion: Some("Ensure the context reference points to a valid agent context".to_string()),
            });
        }

        // Add summary prompt
        combined_messages.push(crate::providers::provider::Message::User {
            content: "Please provide a concise summary of the above conversation, capturing the key points and main topics discussed.".to_string(),
        });

        Ok(combined_messages)
    }

    fn process_context_items(&self, items: &[Value], combined_messages: &mut Vec<crate::providers::provider::Message>) {
        if items[0].is_array() {
            // Handle nested arrays
            for context_array in items {
                if let Value::Array(messages) = context_array {
                    for msg in messages {
                        if let Ok(message) = serde_json::from_value(msg.clone()) {
                            combined_messages.push(message);
                        }
                    }
                }
            }
        } else {
            // Handle flat array
            for msg in items {
                if let Ok(message) = serde_json::from_value(msg.clone()) {
                    combined_messages.push(message);
                }
            }
        }
    }

    async fn execute_compact_operation(
        &self,
        provider: ProviderRef,
        combined_messages: Vec<crate::providers::provider::Message>,
        model_ref: &str,
    ) -> Result<Value, ExecutionError> {
        let dummy_agent = crate::ast::Agent {
            name: "compact".to_string(),
            is_terminal: false,
            properties: vec![crate::ast::AgentProperty::Model {
                value: crate::ast::Value::String(model_ref.to_string()),
                span: crate::ast::Span::new(0, 0, 0, 0),
            }],
            span: crate::ast::Span::new(0, 0, 0, 0),
        };

        let result = provider
            .execute_agent(&dummy_agent, combined_messages.clone(), Vec::new())
            .await
            .map_err(|error| ExecutionError::ProviderError {
                agent: "compact".to_string(),
                message: error.to_string(),
                suggestion: Some("Check provider connectivity".to_string()),
            })?;

        let last_message = result.context.last().ok_or_else(|| ExecutionError::RuntimeError {
            agent: "compact".to_string(),
            message: "No response from compact operation".to_string(),
            suggestion: None,
        })?;

        let compacted_context = vec![last_message.clone()];
        Ok(serde_json::to_value(&compacted_context).unwrap_or(Value::Null))
    }
}

pub struct ExecutionEngine {
    tool_registry: ToolRegistry,
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_registry: ToolRegistry::default(),
        }
    }

    #[must_use]
    pub fn with_tools(tool_registry: ToolRegistry) -> Self {
        Self { tool_registry }
    }

    pub async fn execute_workflow(&self, workflow_path: &str) -> Result<Value, ExecutionError> {
        self.execute_workflow_with_inputs(workflow_path, HashMap::new()).await
    }

    pub async fn execute_workflow_content(&self, workflow_content: &str) -> Result<Value, ExecutionError> {
        self.execute_workflow_from_content(workflow_content, "workflow").await
    }

    pub async fn execute_workflow_from_content(
        &self,
        workflow_content: &str,
        workflow_name: &str,
    ) -> Result<Value, ExecutionError> {
        self.execute_workflow_from_content_with_inputs(workflow_content, workflow_name, HashMap::new())
            .await
    }

    pub async fn execute_workflow_from_content_with_inputs(
        &self,
        workflow_content: &str,
        workflow_name: &str,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, ExecutionError> {
        log::info!("Starting workflow execution: {workflow_name}");
        log::debug!("Workflow inputs: {inputs:?}");

        let builder = AstBuilder::new(workflow_name.to_string());

        let workflow = builder
            .parse(workflow_content)
            .map_err(|error| ExecutionError::RuntimeError {
                agent: "workflow".to_string(),
                message: format!("Failed to parse workflow: {error}"),
                suggestion: Some("Check workflow syntax".to_string()),
            })?;

        log::info!("Workflow parsed successfully");
        log::debug!(
            "Workflow contains {} agents, {} providers",
            workflow.agents.len(),
            workflow.providers.len()
        );

        self.execute_parsed_workflow_with_inputs(&workflow, inputs).await
    }

    pub async fn execute_workflow_with_inputs(
        &self,
        workflow_path: &str,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, ExecutionError> {
        log::info!("Starting workflow execution: {workflow_path}");
        log::debug!("Workflow inputs: {inputs:?}");

        let workflow_content =
            std::fs::read_to_string(workflow_path).map_err(|error| ExecutionError::RuntimeError {
                agent: "workflow".to_string(),
                message: format!("Failed to read workflow file: {error}"),
                suggestion: Some("Check that the file exists and is readable".to_string()),
            })?;

        log::debug!("Workflow file read successfully");

        self.execute_workflow_from_content_with_inputs(&workflow_content, workflow_path, inputs)
            .await
    }

    pub async fn execute_parsed_workflow(&self, workflow: &Workflow) -> Result<Value, ExecutionError> {
        self.execute_parsed_workflow_with_inputs(workflow, HashMap::new()).await
    }

    #[allow(clippy::too_many_lines)]
    pub async fn execute_parsed_workflow_with_inputs(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, ExecutionError> {
        let provider_registry = ProviderRegistry::new();

        for provider in &workflow.providers {
            log::info!("Initializing provider: {}", provider.name);
            log::debug!("Provider models: {:?}", provider.models);

            let provider_instance =
                ProviderFactory::create_provider(provider).map_err(|error| ExecutionError::ProviderError {
                    agent: "workflow".to_string(),
                    message: format!("Failed to create provider '{}': {}", provider.name, error),
                    suggestion: Some("Check provider configuration".to_string()),
                })?;

            provider_registry.register(provider.name.clone(), provider_instance);
            log::info!("Provider '{}' initialized successfully", provider.name);
        }

        self.execute_parsed_workflow_with_inputs_and_registry(workflow, inputs, provider_registry)
            .await
    }

    #[allow(clippy::too_many_lines)]
    pub async fn execute_parsed_workflow_with_inputs_and_registry(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, Value>,
        provider_registry: ProviderRegistry,
    ) -> Result<Value, ExecutionError> {
        // Validate workflow
        self.validate_workflow(workflow)?;

        // Build execution plan
        let execution_levels = self.build_execution_plan(workflow)?;

        // Initialize runtime context with inputs
        let mut runtime_context = self.initialize_runtime_context(inputs);

        // Execute all levels
        self.execute_all_levels(workflow, &execution_levels, &mut runtime_context, &provider_registry)
            .await?;

        // Build and return final output
        self.build_final_output(workflow, &runtime_context, &provider_registry)
            .await
    }

    fn validate_workflow(&self, workflow: &Workflow) -> Result<(), ExecutionError> {
        log::info!("Validating workflow");

        WorkflowValidator::validate(workflow).map_err(|errors| {
            let error_messages: Vec<String> = errors.iter().map(std::string::ToString::to_string).collect();

            ExecutionError::RuntimeError {
                agent: "workflow".to_string(),
                message: format!("Validation errors:\n{}", error_messages.join("\n")),
                suggestion: Some("Fix the validation errors above".to_string()),
            }
        })?;

        log::info!("Workflow validation successful");
        Ok(())
    }

    fn build_execution_plan(&self, workflow: &Workflow) -> Result<Vec<Vec<String>>, ExecutionError> {
        log::info!("Building dependency graph");

        let dependency_graph = DependencyGraph::build(workflow).map_err(|error| ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!("Failed to build dependency graph: {error}"),
            suggestion: Some("Check for circular dependencies".to_string()),
        })?;

        let execution_levels = dependency_graph.get_execution_levels();
        log::info!("Execution levels determined: {execution_levels:?}");

        Ok(execution_levels)
    }

    fn initialize_runtime_context(&self, inputs: HashMap<String, Value>) -> RuntimeContext {
        let runtime_context = RuntimeContext::new();

        for (field_name, value) in inputs {
            runtime_context.set_input_value(field_name, value);
        }

        runtime_context
    }

    async fn execute_all_levels(
        &self,
        workflow: &Workflow,
        execution_levels: &[Vec<String>],
        runtime_context: &mut RuntimeContext,
        provider_registry: &ProviderRegistry,
    ) -> Result<(), ExecutionError> {
        for level in execution_levels {
            self.execute_agent_level(workflow, level, runtime_context, provider_registry)
                .await?;
        }

        log::info!("All agents executed successfully");
        Ok(())
    }

    async fn execute_agent_level(
        &self,
        workflow: &Workflow,
        level: &[String],
        runtime_context: &mut RuntimeContext,
        provider_registry: &ProviderRegistry,
    ) -> Result<(), ExecutionError> {
        log::info!("Executing level with {} agent(s): {:?}", level.len(), level);

        let mut tasks = Vec::new();

        for agent_name in level {
            let agent = workflow
                .agents
                .iter()
                .find(|agent| &agent.name == agent_name)
                .ok_or_else(|| ExecutionError::RuntimeError {
                    agent: agent_name.clone(),
                    message: "Agent not found in workflow".to_string(),
                    suggestion: None,
                })?;

            let task = self.spawn_agent_task(agent, runtime_context, provider_registry)?;
            tasks.push(task);
        }

        self.collect_agent_results(tasks, runtime_context).await
    }

    fn spawn_agent_task(
        &self,
        agent: &crate::ast::Agent,
        runtime_context: &RuntimeContext,
        provider_registry: &ProviderRegistry,
    ) -> Result<AgentTaskHandle, ExecutionError> {
        let agent_clone = agent.clone();
        let provider_registry_clone = provider_registry.clone();
        let runtime_context_clone = runtime_context.clone();
        let tool_registry_clone = self.tool_registry.clone();

        let task = tokio::task::spawn(async move {
            let provider = Self::get_provider_for_agent(&agent_clone, &provider_registry_clone)?;
            let provider_clone = provider.clone();
            let orchestrator = AgentOrchestrator::with_tools(provider, tool_registry_clone.clone());

            let initial_context = Self::extract_initial_context(&agent_clone, &runtime_context_clone)?;

            let for_each_property = Self::extract_for_each_property(&agent_clone);

            if let Some((collection_value, iteration_var)) = for_each_property {
                Self::execute_for_each_agent(
                    agent_clone.clone(),
                    collection_value,
                    iteration_var,
                    initial_context,
                    runtime_context_clone,
                    provider_clone,
                    tool_registry_clone,
                )
                .await
            } else {
                let (output, context) = orchestrator
                    .execute_agent(&agent_clone, initial_context, &runtime_context_clone)
                    .await?;

                Ok((agent_clone.name.clone(), output, context))
            }
        });

        Ok(task)
    }

    async fn collect_agent_results(
        &self,
        tasks: Vec<AgentTaskHandle>,
        runtime_context: &mut RuntimeContext,
    ) -> Result<(), ExecutionError> {
        for task in tasks {
            let (agent_name, output, context) = task.await.map_err(|error| ExecutionError::RuntimeError {
                agent: "parallel_execution".to_string(),
                message: format!("Failed to execute agent in parallel: {error}"),
                suggestion: None,
            })??;

            runtime_context.set_agent_output(agent_name.clone(), output);
            runtime_context.set_agent_context(agent_name, context);
        }

        Ok(())
    }

    fn extract_initial_context(
        agent: &crate::ast::Agent,
        runtime_context: &RuntimeContext,
    ) -> Result<Vec<crate::providers::provider::Message>, ExecutionError> {
        let context_property = agent.properties.iter().find_map(|prop| {
            if let crate::ast::AgentProperty::Context { value, .. } = prop {
                Some(value)
            } else {
                None
            }
        });

        if let Some(context_value) = context_property {
            let resolved = runtime_context.resolve_value(context_value)?;

            if let Value::Array(messages) = resolved {
                Ok(messages
                    .into_iter()
                    .filter_map(|msg| serde_json::from_value(msg).ok())
                    .collect())
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn extract_for_each_property(agent: &crate::ast::Agent) -> Option<(&crate::ast::Value, &String)> {
        agent.properties.iter().find_map(|prop| {
            if let crate::ast::AgentProperty::ForEach {
                collection, identifier, ..
            } = prop
            {
                Some((collection, identifier))
            } else {
                None
            }
        })
    }

    async fn execute_for_each_agent(
        agent: crate::ast::Agent,
        collection_value: &crate::ast::Value,
        iteration_var: &str,
        initial_context: Vec<crate::providers::provider::Message>,
        runtime_context: RuntimeContext,
        provider: ProviderRef,
        tool_registry: ToolRegistry,
    ) -> Result<(String, Value, Vec<crate::providers::provider::Message>), ExecutionError> {
        let collection = runtime_context.resolve_value(collection_value)?;

        let Value::Array(items) = collection else {
            return Err(ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: "for_each collection must be an array".to_string(),
                suggestion: Some("Ensure the collection resolves to an array".to_string()),
            });
        };

        let mut iteration_tasks = Vec::new();

        for (iteration_index, item) in items.into_iter().enumerate() {
            let agent_clone = agent.clone();
            let initial_context_clone = initial_context.clone();
            let iteration_context = runtime_context.clone();
            iteration_context.set_input_value(iteration_var.to_string(), item);
            let provider_for_iteration = provider.clone();
            let tool_registry_for_iteration = tool_registry.clone();

            let iteration_task = tokio::task::spawn(async move {
                let orchestrator = AgentOrchestrator::with_tools(provider_for_iteration, tool_registry_for_iteration);
                let result = orchestrator
                    .execute_agent(&agent_clone, initial_context_clone, &iteration_context)
                    .await;

                (iteration_index, result)
            });

            iteration_tasks.push(iteration_task);
        }

        let mut iteration_results = Vec::new();
        for iteration_task in iteration_tasks {
            let (iteration_index, result) = iteration_task.await.map_err(|error| ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: format!("Failed to execute for_each iteration: {error}"),
                suggestion: None,
            })?;

            let (output, context) = result?;
            iteration_results.push((iteration_index, output, context));
        }

        iteration_results.sort_by_key(|(index, _, _)| *index);

        let mut results = Vec::new();
        let mut all_contexts = Vec::new();
        for (_, output, context) in iteration_results {
            results.push(output);
            all_contexts.extend(context);
        }

        Ok((agent.name.clone(), Value::Array(results), all_contexts))
    }

    async fn build_final_output(
        &self,
        workflow: &Workflow,
        runtime_context: &RuntimeContext,
        provider_registry: &ProviderRegistry,
    ) -> Result<Value, ExecutionError> {
        let terminal_agents: Vec<_> = workflow.agents.iter().filter(|agent| agent.is_terminal).collect();

        if terminal_agents.is_empty() && workflow.output.is_none() {
            log::info!("No terminal agents or output block defined, returning null");
            return Ok(Value::Null);
        }

        log::info!("Building final output");

        let mut result = serde_json::Map::new();

        // Process terminal agents
        self.process_terminal_agents(&terminal_agents, runtime_context, &mut result)?;

        // Process output block
        if let Some(output_block) = &workflow.output {
            self.process_output_block(output_block, runtime_context, provider_registry, &mut result)
                .await?;
        }

        log::info!("Workflow execution completed successfully");
        log::debug!("Final output: {result:?}");

        Ok(Value::Object(result))
    }

    fn process_terminal_agents(
        &self,
        terminal_agents: &[&crate::ast::Agent],
        runtime_context: &RuntimeContext,
        result: &mut serde_json::Map<String, Value>,
    ) -> Result<(), ExecutionError> {
        for terminal_agent in terminal_agents {
            if let Ok(output) =
                runtime_context.resolve_value(&crate::ast::Value::Reference(crate::ast::Reference::Agent {
                    agent: terminal_agent.name.clone(),
                    field: "_output".to_string(),
                }))
            {
                result.insert(terminal_agent.name.clone(), output);
            }
        }

        Ok(())
    }

    async fn process_output_block(
        &self,
        output_block: &crate::ast::OutputBlock,
        runtime_context: &RuntimeContext,
        provider_registry: &ProviderRegistry,
        result: &mut serde_json::Map<String, Value>,
    ) -> Result<(), ExecutionError> {
        for field in &output_block.fields {
            let value = if let crate::ast::Value::FunctionCall(function_call) = &field.value {
                if function_call.name == "compact" {
                    self.execute_compact_function(function_call, runtime_context, provider_registry)
                        .await?
                } else {
                    runtime_context.resolve_value(&field.value)?
                }
            } else {
                runtime_context.resolve_value(&field.value)?
            };

            result.insert(field.name.clone(), value);
        }

        Ok(())
    }

    fn get_provider_for_agent(
        agent: &crate::ast::Agent,
        provider_registry: &ProviderRegistry,
    ) -> Result<ProviderRef, ExecutionError> {
        for property in &agent.properties {
            if let crate::ast::AgentProperty::Model { value, .. } = property {
                let model_ref = match value {
                    crate::ast::Value::String(string) => string.clone(),
                    crate::ast::Value::Interpolated(string) => string.clone(),
                    _ => continue,
                };

                let (provider, _model) = provider_registry.get_model_provider(&model_ref).map_err(|error| {
                    ExecutionError::ProviderError {
                        agent: agent.name.clone(),
                        message: error.to_string(),
                        suggestion: Some("Check provider and model configuration".to_string()),
                    }
                })?;

                return Ok(provider);
            }
        }

        Err(ExecutionError::RuntimeError {
            agent: agent.name.clone(),
            message: "No model specified for agent".to_string(),
            suggestion: Some("Add a 'model' property to the agent".to_string()),
        })
    }

    async fn execute_compact_function(
        &self,
        function_call: &crate::ast::FunctionCall,
        runtime_context: &RuntimeContext,
        provider_registry: &ProviderRegistry,
    ) -> Result<Value, ExecutionError> {
        let handler = CompactFunctionHandler::new(provider_registry);
        handler.execute(function_call, runtime_context).await
    }
}
