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
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_workflow(&self, workflow_path: &str) -> Result<Value, ExecutionError> {
        self.execute_workflow_with_inputs(workflow_path, HashMap::new()).await
    }

    pub async fn execute_workflow_with_inputs(
        &self,
        workflow_path: &str,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, ExecutionError> {
        let workflow_content =
            std::fs::read_to_string(workflow_path).map_err(|error| ExecutionError::RuntimeError {
                agent: "workflow".to_string(),
                message: format!("Failed to read workflow file: {}", error),
                suggestion: Some("Check that the file exists and is readable".to_string()),
            })?;

        let builder = AstBuilder::new(workflow_path.to_string());

        let workflow = builder
            .parse(&workflow_content)
            .map_err(|error| ExecutionError::RuntimeError {
                agent: "workflow".to_string(),
                message: format!("Failed to parse workflow: {}", error),
                suggestion: Some("Check workflow syntax".to_string()),
            })?;

        self.execute_parsed_workflow_with_inputs(&workflow, inputs).await
    }

    pub async fn execute_parsed_workflow(&self, workflow: &Workflow) -> Result<Value, ExecutionError> {
        self.execute_parsed_workflow_with_inputs(workflow, HashMap::new()).await
    }

    pub async fn execute_parsed_workflow_with_inputs(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, ExecutionError> {
        WorkflowValidator::validate(workflow).map_err(|errors| ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!(
                "Validation errors:\n{}",
                errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
            ),
            suggestion: Some("Fix the validation errors above".to_string()),
        })?;

        let mut provider_registry = ProviderRegistry::new();

        for provider in &workflow.providers {
            let provider_instance =
                ProviderFactory::create_provider(provider).map_err(|error| ExecutionError::ProviderError {
                    agent: "workflow".to_string(),
                    message: format!("Failed to create provider '{}': {}", provider.name, error),
                    suggestion: Some("Check provider configuration".to_string()),
                })?;

            provider_registry.register(provider.name.clone(), provider_instance);
        }

        let dependency_graph = DependencyGraph::build(workflow).map_err(|error| ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!("Failed to build dependency graph: {}", error),
            suggestion: Some("Check for circular dependencies".to_string()),
        })?;

        let execution_order = dependency_graph.topological_order();

        let mut runtime_context = RuntimeContext::new();

        for (field_name, value) in inputs {
            runtime_context.set_input_value(field_name, value);
        }

        for agent_name in execution_order {
            let agent = workflow
                .agents
                .iter()
                .find(|agent| agent.name == agent_name)
                .ok_or_else(|| ExecutionError::RuntimeError {
                    agent: agent_name.clone(),
                    message: "Agent not found in workflow".to_string(),
                    suggestion: None,
                })?;

            let provider = Self::get_provider_for_agent(agent, &provider_registry)?;

            let orchestrator = AgentOrchestrator::new(provider);

            let context_property = agent.properties.iter().find_map(|prop| {
                if let crate::ast::AgentProperty::Context { value, .. } = prop {
                    Some(value)
                } else {
                    None
                }
            });

            let initial_context = if let Some(context_value) = context_property {
                let resolved = runtime_context.resolve_value(context_value)?;

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

            let for_each_property = agent.properties.iter().find_map(|prop| {
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
                let collection = runtime_context.resolve_value(collection_value)?;

                let items = if let Value::Array(array) = collection {
                    array
                } else {
                    return Err(ExecutionError::RuntimeError {
                        agent: agent_name.clone(),
                        message: "for_each collection must be an array".to_string(),
                        suggestion: Some("Ensure the collection resolves to an array".to_string()),
                    });
                };

                let mut results = Vec::new();

                for item in items {
                    let mut iteration_context = runtime_context.clone();
                    iteration_context.set_input_value(iteration_var.clone(), item);

                    let (output, _context) = orchestrator
                        .execute_agent(agent, initial_context.clone(), &iteration_context)
                        .await?;

                    results.push(output);
                }

                runtime_context.set_agent_output(agent_name.clone(), Value::Array(results));
                runtime_context.set_agent_context(agent_name.clone(), Vec::new());
            } else {
                let (output, context) = orchestrator
                    .execute_agent(agent, initial_context, &runtime_context)
                    .await?;

                runtime_context.set_agent_output(agent_name.clone(), output);
                runtime_context.set_agent_context(agent_name.clone(), context);
            }
        }

        let terminal_agents: Vec<_> = workflow.agents.iter().filter(|agent| agent.is_terminal).collect();

        if terminal_agents.is_empty() && workflow.output.is_none() {
            return Ok(Value::Null);
        }

        if terminal_agents.len() == 1 && workflow.output.is_none() {
            let terminal_agent = terminal_agents[0];
            return runtime_context
                .resolve_value(&crate::ast::Value::Reference(crate::ast::Reference::Agent {
                    agent: terminal_agent.name.clone(),
                    field: "_output".to_string(),
                }))
                .or_else(|_| Ok(Value::Null));
        }

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

        let contexts = if let Value::Array(array) = resolved_context {
            array
        } else {
            vec![resolved_context]
        };

        let mut combined_messages = Vec::new();

        for context in contexts {
            if let Value::Array(messages) = context {
                for msg in messages {
                    if let Ok(message) = serde_json::from_value(msg) {
                        combined_messages.push(message);
                    }
                }
            }
        }

        let summary_prompt = "Please provide a concise summary of the above conversation, capturing the key points and main topics discussed.";

        combined_messages.push(crate::providers::provider::Message::User {
            content: summary_prompt.to_string(),
        });

        let dummy_agent = crate::ast::Agent {
            name: "compact".to_string(),
            is_terminal: false,
            properties: vec![],
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

        Ok(serde_json::to_value(&result.context).unwrap_or(Value::Null))
    }
}
