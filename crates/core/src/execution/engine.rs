use crate::ast::Workflow;
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::execution::orchestrator::AgentOrchestrator;
use crate::parser::{AstBuilder, DependencyGraph};
use crate::providers::{ProviderFactory, ProviderRef, ProviderRegistry};
use crate::validation::WorkflowValidator;
use serde_json::Value;
use std::collections::HashMap;

pub struct ExecutionEngine;

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub async fn execute_workflow(&self, workflow_path: &str) -> Result<Value, ExecutionError> {
        self.execute_workflow_with_inputs(workflow_path, HashMap::new()).await
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
        let mut provider_registry = ProviderRegistry::new();

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

        log::info!("Building dependency graph");

        let dependency_graph = DependencyGraph::build(workflow).map_err(|error| ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!("Failed to build dependency graph: {error}"),
            suggestion: Some("Check for circular dependencies".to_string()),
        })?;

        let execution_levels = dependency_graph.get_execution_levels();
        log::info!("Execution levels determined: {execution_levels:?}");

        let mut runtime_context = RuntimeContext::new();

        for (field_name, value) in inputs {
            runtime_context.set_input_value(field_name, value);
        }

        for level in execution_levels {
            log::info!("Executing level with {} agent(s): {:?}", level.len(), level);

            let mut tasks = Vec::new();

            for agent_name in &level {
                let agent = workflow
                    .agents
                    .iter()
                    .find(|agent| &agent.name == agent_name)
                    .ok_or_else(|| ExecutionError::RuntimeError {
                        agent: agent_name.clone(),
                        message: "Agent not found in workflow".to_string(),
                        suggestion: None,
                    })?;

                let agent_clone = agent.clone();
                let provider_registry_clone = provider_registry.clone();
                let runtime_context_clone = runtime_context.clone();

                let task = tokio::task::spawn(async move {
                    let provider = Self::get_provider_for_agent(&agent_clone, &provider_registry_clone)?;
                    let provider_clone = provider.clone();
                    let orchestrator = AgentOrchestrator::new(provider);

                    let context_property = agent_clone.properties.iter().find_map(|prop| {
                        if let crate::ast::AgentProperty::Context { value, .. } = prop {
                            Some(value)
                        } else {
                            None
                        }
                    });

                    let initial_context = if let Some(context_value) = context_property {
                        let resolved = runtime_context_clone.resolve_value(context_value)?;

                        if let Value::Array(messages) = resolved {
                            messages
                                .into_iter()
                                .filter_map(|msg| serde_json::from_value(msg).ok())
                                .collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                    let for_each_property = agent_clone.properties.iter().find_map(|prop| {
                        if let crate::ast::AgentProperty::ForEach {
                            collection, identifier, ..
                        } = prop
                        {
                            Some((collection, identifier))
                        } else {
                            None
                        }
                    });

                    if let Some((collection_value, iteration_var)) = for_each_property {
                        let collection = runtime_context_clone.resolve_value(collection_value)?;

                        let Value::Array(items) = collection else {
                            return Err(ExecutionError::RuntimeError {
                                agent: agent_clone.name.clone(),
                                message: "for_each collection must be an array".to_string(),
                                suggestion: Some("Ensure the collection resolves to an array".to_string()),
                            });
                        };

                        let _total_items = items.len();
                        let mut iteration_tasks = Vec::new();

                        for (iteration_index, item) in items.into_iter().enumerate() {
                            let agent_clone_inner = agent_clone.clone();
                            let initial_context_clone = initial_context.clone();
                            let mut iteration_context = runtime_context_clone.clone();
                            iteration_context.set_input_value(iteration_var.clone(), item);
                            let provider_for_iteration = provider_clone.clone();

                            let iteration_task = tokio::task::spawn(async move {
                                let orchestrator_inner = AgentOrchestrator::new(provider_for_iteration);
                                let result = orchestrator_inner
                                    .execute_agent(&agent_clone_inner, initial_context_clone, &iteration_context)
                                    .await;

                                (iteration_index, result)
                            });

                            iteration_tasks.push(iteration_task);
                        }

                        let mut iteration_results = Vec::new();
                        for iteration_task in iteration_tasks {
                            let (iteration_index, result) =
                                iteration_task.await.map_err(|error| ExecutionError::RuntimeError {
                                    agent: agent_clone.name.clone(),
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

                        Ok((agent_clone.name.clone(), Value::Array(results), all_contexts))
                    } else {
                        let (output, context) = orchestrator
                            .execute_agent(&agent_clone, initial_context, &runtime_context_clone)
                            .await?;

                        Ok((agent_clone.name.clone(), output, context))
                    }
                });

                tasks.push(task);
            }

            for task in tasks {
                let (agent_name, output, context) = task.await.map_err(|error| ExecutionError::RuntimeError {
                    agent: "parallel_execution".to_string(),
                    message: format!("Failed to execute agent in parallel: {error}"),
                    suggestion: None,
                })??;

                runtime_context.set_agent_output(agent_name.clone(), output);
                runtime_context.set_agent_context(agent_name, context);
            }
        }

        log::info!("All agents executed successfully");

        let terminal_agents: Vec<_> = workflow.agents.iter().filter(|agent| agent.is_terminal).collect();

        if terminal_agents.is_empty() && workflow.output.is_none() {
            log::info!("No terminal agents or output block defined, returning null");
            return Ok(Value::Null);
        }

        log::info!("Building final output");

        let mut result = serde_json::Map::new();

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

        if let Some(output_block) = &workflow.output {
            for field in &output_block.fields {
                let value = if let crate::ast::Value::FunctionCall(function_call) = &field.value {
                    if function_call.name == "compact" {
                        self.execute_compact_function(function_call, &runtime_context, &provider_registry)
                            .await?
                    } else {
                        runtime_context.resolve_value(&field.value)?
                    }
                } else {
                    runtime_context.resolve_value(&field.value)?
                };

                result.insert(field.name.clone(), value);
            }
        }

        log::info!("Workflow execution completed successfully");
        log::debug!("Final output: {result:?}");

        Ok(Value::Object(result))
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
        let model_value = function_call
            .arguments
            .get("model")
            .ok_or_else(|| ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "compact function requires 'model' argument".to_string(),
                suggestion: Some("Provide a model like 'ollama1/qwen3:8b'".to_string()),
            })?;

        let model_ref = match model_value {
            crate::ast::Value::String(string) => string.clone(),
            crate::ast::Value::Interpolated(string) => string.clone(),
            _ => {
                return Err(ExecutionError::RuntimeError {
                    agent: "compact".to_string(),
                    message: "model must be a string".to_string(),
                    suggestion: None,
                })
            }
        };

        let (provider, _model) =
            provider_registry
                .get_model_provider(&model_ref)
                .map_err(|error| ExecutionError::ProviderError {
                    agent: "compact".to_string(),
                    message: error.to_string(),
                    suggestion: Some("Check provider and model configuration".to_string()),
                })?;

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

            if items[0].is_array() {
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
                for msg in items {
                    if let Ok(message) = serde_json::from_value(msg.clone()) {
                        combined_messages.push(message);
                    }
                }
            }
        }

        if combined_messages.is_empty() {
            return Err(ExecutionError::RuntimeError {
                agent: "compact".to_string(),
                message: "No messages found in context to compact".to_string(),
                suggestion: Some("Ensure the context reference points to a valid agent context".to_string()),
            });
        }

        let summary_prompt = "Please provide a concise summary of the above conversation, capturing the key points and main topics discussed.";

        combined_messages.push(crate::providers::provider::Message::User {
            content: summary_prompt.to_string(),
        });

        let dummy_agent = crate::ast::Agent {
            name: "compact".to_string(),
            is_terminal: false,
            properties: vec![crate::ast::AgentProperty::Model {
                value: crate::ast::Value::String(model_ref.clone()),
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
