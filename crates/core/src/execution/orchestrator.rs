use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::info;
use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use rayon::prelude::*;
use serde_json::{Map, Value};

use crate::ast::{AgentDefinition, ContextSource, Expression, WorkflowDocument};
use crate::execution::engine::{execute_agent, summarize_context, AgentExecutionResult};
use crate::execution::error::ExecutionError;
use crate::providers::provider::ProviderModelConfig;
use crate::providers::registry::{resolve_model_config, ProviderRegistry};
use crate::utils::template::interpolate_template;

pub async fn execute_workflow(
    document: &WorkflowDocument,
    registry: &ProviderRegistry,
) -> Result<Value, ExecutionError> {
    let ordered_agents = topologically_sorted_agents(document)?;
    let mut results = HashMap::<String, AgentExecutionResult>::new();

    info!("starting workflow execution with {} agents", ordered_agents.len());

    for agent in ordered_agents {
        info!("processing agent: {}", agent.name);
        let model = agent
            .model
            .as_ref()
            .map(|reference| {
                let provider_definition = document
                    .providers
                    .iter()
                    .find(|provider| provider.name == reference.provider)
                    .expect("validated provider exists");
                resolve_model_config(provider_definition, &reference.model)
            })
            .unwrap_or(ProviderModelConfig {
                provider_name: "noop".into(),
                model_name: "noop".into(),
                api_endpoint: None,
            });

        if model.provider_name == "noop" {
            results.insert(
                agent.name.clone(),
                AgentExecutionResult {
                    status: "success".into(),
                    output: Value::String(String::new()),
                    transcript: Vec::new(),
                    context: Default::default(),
                },
            );
            continue;
        }

        let provider_definition = document
            .providers
            .iter()
            .find(|provider| provider.name == model.provider_name)
            .expect("validated provider exists");
        let provider = registry.get(&provider_definition.driver)?;
        let mut materialized_agent = agent.clone();
        materialize_prompts(&mut materialized_agent, &results)?;
        materialize_context(document, &results, &mut materialized_agent, registry).await?;
        info!("executing workflow agent: {}", agent.name);
        let result = if materialized_agent.for_each.is_some() {
            execute_for_each(&materialized_agent, document, provider, model).await?
        } else {
            execute_agent(&materialized_agent, document, provider, model).await?
        };
        results.insert(agent.name.clone(), result);
    }

    collect_terminal_outputs(document, &results)
}

fn topologically_sorted_agents(document: &WorkflowDocument) -> Result<Vec<&AgentDefinition>, ExecutionError> {
    let mut graph = DiGraph::<&str, ()>::new();
    let mut nodes = HashMap::new();

    for agent in &document.agents {
        let index = graph.add_node(agent.name.as_str());
        nodes.insert(agent.name.as_str(), index);
    }

    for agent in &document.agents {
        for dependency in collect_dependencies(agent) {
            if let Some(dep_index) = nodes.get(dependency.as_str()) {
                let agent_index = nodes[agent.name.as_str()];
                graph.add_edge(*dep_index, agent_index, ());
            }
        }
    }

    let order = toposort(&graph, None).map_err(|cycle| ExecutionError::DependencyCycle {
        node: graph[cycle.node_id()].to_owned(),
    })?;

    let order_names = order.into_iter().map(|idx| graph[idx]).collect::<Vec<_>>();
    let by_name = document
        .agents
        .iter()
        .map(|agent| (agent.name.as_str(), agent))
        .collect::<HashMap<_, _>>();

    Ok(order_names
        .into_par_iter()
        .map(|name| by_name[name])
        .collect::<Vec<_>>())
}

fn materialize_prompts(
    agent: &mut AgentDefinition,
    results: &HashMap<String, AgentExecutionResult>,
) -> Result<(), ExecutionError> {
    if agent.for_each.is_some() {
        if let Some(for_each_binding) = &agent.for_each {
            let materialized_collection = materialize_expression(&for_each_binding.collection, results)?;
            agent.for_each = Some(crate::ast::ForEachBinding {
                collection: Box::new(materialized_collection),
                binding: for_each_binding.binding.clone(),
            });
        }
        return Ok(());
    }

    if let Some(prompt_expr) = &agent.prompt {
        let materialized = materialize_expression(prompt_expr, results)?;
        agent.prompt = Some(materialized);
    }

    Ok(())
}

fn json_value_to_expression(value: &Value) -> Result<Expression, ExecutionError> {
    match value {
        Value::String(s) => Ok(Expression::String(s.clone())),
        Value::Number(n) => Ok(Expression::Number(n.as_f64().unwrap_or(0.0))),
        Value::Bool(b) => Ok(Expression::Boolean(*b)),
        Value::Null => Ok(Expression::Null),
        Value::Array(items) => {
            let expressions = items
                .iter()
                .map(json_value_to_expression)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::Array(expressions))
        }
        Value::Object(map) => {
            let mut object = indexmap::IndexMap::new();
            for (key, val) in map {
                object.insert(key.clone(), json_value_to_expression(val)?);
            }
            Ok(Expression::Object(object))
        }
    }
}

fn materialize_expression(
    expr: &Expression,
    results: &HashMap<String, AgentExecutionResult>,
) -> Result<Expression, ExecutionError> {
    match expr {
        Expression::String(s) | Expression::MultilineString(s) | Expression::InterpolatedString(s) => {
            let variables = build_variable_map(results);
            log::debug!("Materializing template with {} variables", variables.len());
            for (key, value) in &variables {
                log::debug!("  Variable '{}': {:?}", key, value);
            }
            log::debug!("Template: {}", s);
            let interpolated =
                interpolate_template(s, &variables).map_err(|e| ExecutionError::UnsupportedExpression {
                    expression: format!("template interpolation failed: {}", e),
                })?;
            Ok(Expression::String(interpolated))
        }
        Expression::Array(items) => {
            let materialized_items = items
                .iter()
                .map(|item| materialize_expression(item, results))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::Array(materialized_items))
        }
        Expression::Object(values) => {
            let materialized_values = values
                .iter()
                .map(|(k, v)| Ok((k.clone(), materialize_expression(v, results)?)))
                .collect::<Result<indexmap::IndexMap<_, _>, ExecutionError>>()?;
            Ok(Expression::Object(materialized_values))
        }
        Expression::Reference(reference) => {
            let value = resolve_reference(reference, results)?;
            json_value_to_expression(&value)
        }
        Expression::FunctionCall(function_call) => {
            if function_call.name == "file" {
                evaluate_file_function(function_call, results)
            } else {
                Err(ExecutionError::UnsupportedExpression {
                    expression: format!("unknown function: {}", function_call.name),
                })
            }
        }
        other => Ok(other.clone()),
    }
}

fn evaluate_file_function(
    function_call: &crate::ast::FunctionCall,
    results: &HashMap<String, AgentExecutionResult>,
) -> Result<Expression, ExecutionError> {
    let file_path = match &*function_call.target {
        Expression::String(path) => path.clone(),
        _ => {
            return Err(ExecutionError::UnsupportedExpression {
                expression: "file() target must be a string".into(),
            })
        }
    };

    let mut variables = HashMap::new();
    for (key, value_expr) in &function_call.arguments {
        let materialized = materialize_expression(value_expr, results)?;
        let value = match materialized {
            Expression::String(s) => Value::String(s),
            Expression::Number(n) => serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Expression::Boolean(b) => Value::Bool(b),
            Expression::Null => Value::Null,
            _ => {
                return Err(ExecutionError::UnsupportedExpression {
                    expression: format!("unsupported file() argument type for key: {}", key),
                })
            }
        };
        variables.insert(key.clone(), value);
    }

    let content = crate::utils::template::read_and_interpolate_file(&file_path, &variables).map_err(|e| {
        ExecutionError::UnsupportedExpression {
            expression: format!("file() evaluation failed: {}", e),
        }
    })?;

    Ok(Expression::String(content))
}

fn build_variable_map(results: &HashMap<String, AgentExecutionResult>) -> HashMap<String, Value> {
    let mut variables = HashMap::new();

    for (agent_name, result) in results {
        variables.insert(agent_name.clone(), result.output.clone());
    }

    variables
}

fn resolve_reference(
    reference: &crate::ast::Reference,
    results: &HashMap<String, AgentExecutionResult>,
) -> Result<Value, ExecutionError> {
    if reference.segments.is_empty() {
        return Err(ExecutionError::InvalidContextReference {
            reference: reference.as_string(),
        });
    }

    let (agent_name, field_start) = if reference.segments[0] == "agent" {
        (
            reference
                .segments
                .get(1)
                .ok_or_else(|| ExecutionError::InvalidContextReference {
                    reference: reference.as_string(),
                })?,
            2,
        )
    } else {
        (&reference.segments[0], 1)
    };

    let result = results
        .get(agent_name)
        .ok_or_else(|| ExecutionError::MissingAgentResult {
            agent: agent_name.clone(),
        })?;

    let mut current = result.output.clone();

    for segment in &reference.segments[field_start..] {
        current = match current {
            Value::Object(map) => map
                .get(segment)
                .cloned()
                .ok_or_else(|| ExecutionError::InvalidContextReference {
                    reference: reference.as_string(),
                })?,
            Value::Array(arr) => {
                let index: usize = segment.parse().map_err(|_| ExecutionError::InvalidContextReference {
                    reference: reference.as_string(),
                })?;
                arr.get(index)
                    .cloned()
                    .ok_or_else(|| ExecutionError::InvalidContextReference {
                        reference: reference.as_string(),
                    })?
            }
            _ => {
                return Err(ExecutionError::InvalidContextReference {
                    reference: reference.as_string(),
                })
            }
        };
    }

    Ok(current)
}

async fn materialize_context(
    document: &WorkflowDocument,
    results: &HashMap<String, AgentExecutionResult>,
    agent: &mut AgentDefinition,
    registry: &ProviderRegistry,
) -> Result<(), ExecutionError> {
    let context = match &agent.context {
        Some(context) => context.clone(),
        None => return Ok(()),
    };

    let reference = match &context {
        ContextSource::Full(reference) | ContextSource::Summary(reference) => reference,
    };
    let source_agent_name = reference
        .segments
        .get(1)
        .ok_or_else(|| ExecutionError::InvalidContextReference {
            reference: reference.as_string(),
        })?;
    let source_result = results
        .get(source_agent_name)
        .ok_or_else(|| ExecutionError::MissingAgentResult {
            agent: source_agent_name.clone(),
        })?;

    let context_text = match context {
        ContextSource::Full(_) => source_result.context.messages.join("\n"),
        ContextSource::Summary(_) => {
            if let Some(summary) = &source_result.context.summary {
                summary.clone()
            } else {
                let source_agent = document
                    .agents
                    .iter()
                    .find(|candidate| candidate.name == *source_agent_name)
                    .ok_or_else(|| ExecutionError::MissingAgentResult {
                        agent: source_agent_name.clone(),
                    })?;
                let source_provider_name = source_agent
                    .model
                    .as_ref()
                    .ok_or_else(|| ExecutionError::MissingModel {
                        agent: source_agent_name.clone(),
                    })?
                    .provider
                    .clone();
                let source_provider_def = document
                    .providers
                    .iter()
                    .find(|provider| provider.name == source_provider_name)
                    .ok_or_else(|| ExecutionError::MissingProviderDefinition {
                        provider: source_provider_name.clone(),
                    })?;
                let provider = registry.get(&source_provider_def.driver)?;
                summarize_context(document, source_agent_name, provider, &source_result.context).await?
            }
        }
    };

    if let Some(prompt) = agent.prompt.take() {
        agent.prompt = Some(inject_context(prompt, &context_text));
    }

    Ok(())
}

async fn execute_for_each(
    agent: &AgentDefinition,
    document: &WorkflowDocument,
    provider: Arc<dyn crate::providers::provider::Provider>,
    model: ProviderModelConfig,
) -> Result<AgentExecutionResult, ExecutionError> {
    let binding = agent.for_each.as_ref().expect("for_each execution requires binding");
    let items = evaluate_collection(&binding.collection)?;

    let mut outputs = Vec::with_capacity(items.len());
    let mut transcript = Vec::new();
    let mut combined_context = Vec::new();

    for item in items {
        let mut iteration_agent = agent.clone();
        if let Some(prompt) = iteration_agent.prompt.take() {
            iteration_agent.prompt = Some(apply_binding(prompt, &binding.binding, &item));
        }
        iteration_agent.for_each = None;
        let result = execute_agent(&iteration_agent, document, provider.clone(), model.clone()).await?;
        transcript.extend(result.transcript.clone());
        combined_context.extend(result.context.messages);
        outputs.push(result.output);
    }

    Ok(AgentExecutionResult {
        status: "success".into(),
        output: Value::Array(outputs),
        transcript,
        context: crate::execution::engine::AgentRuntimeContext {
            messages: combined_context,
            summary: None,
        },
    })
}

fn evaluate_collection(expression: &Expression) -> Result<Vec<Value>, ExecutionError> {
    match expression {
        Expression::Array(items) => items.iter().map(expression_to_json).collect(),
        other => Err(ExecutionError::InvalidForEachCollection {
            actual: format!("{other:?}"),
        }),
    }
}

fn expression_to_json(expression: &Expression) -> Result<Value, ExecutionError> {
    Ok(match expression {
        Expression::String(value)
        | Expression::MultilineString(value)
        | Expression::InterpolatedString(value)
        | Expression::Identifier(value) => Value::String(value.clone()),
        Expression::Number(value) => serde_json::Number::from_f64(*value).map(Value::Number).ok_or_else(|| {
            ExecutionError::InvalidNumericValue {
                value: value.to_string(),
            }
        })?,
        Expression::Boolean(value) => Value::Bool(*value),
        Expression::Null => Value::Null,
        Expression::Array(items) => Value::Array(items.iter().map(expression_to_json).collect::<Result<Vec<_>, _>>()?),
        Expression::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), expression_to_json(value)?)))
                .collect::<Result<Map<String, Value>, ExecutionError>>()?,
        ),
        other => {
            return Err(ExecutionError::UnsupportedExpression {
                expression: format!("{other:?}"),
            })
        }
    })
}

fn apply_binding(expression: Expression, binding: &str, item: &Value) -> Expression {
    match expression {
        Expression::String(value) => Expression::String(replace_binding(&value, binding, item)),
        Expression::MultilineString(value) => Expression::MultilineString(replace_binding(&value, binding, item)),
        Expression::InterpolatedString(value) => Expression::InterpolatedString(replace_binding(&value, binding, item)),
        Expression::Array(items) => Expression::Array(
            items
                .into_iter()
                .map(|item_expr| apply_binding(item_expr, binding, item))
                .collect(),
        ),
        Expression::Object(values) => Expression::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, apply_binding(value, binding, item)))
                .collect(),
        ),
        Expression::FunctionCall(mut function_call) => {
            function_call.target = Box::new(apply_binding(*function_call.target, binding, item));
            function_call.arguments = function_call
                .arguments
                .into_iter()
                .map(|(key, value)| (key, apply_binding(value, binding, item)))
                .collect();
            Expression::FunctionCall(function_call)
        }
        other => other,
    }
}

fn inject_context(expression: Expression, context_text: &str) -> Expression {
    match expression {
        Expression::String(value) => Expression::String(format!("{value}\n\nContext:\n{context_text}")),
        Expression::MultilineString(value) => {
            Expression::MultilineString(format!("{value}\n\nContext:\n{context_text}"))
        }
        other => other,
    }
}

fn replace_binding(template: &str, binding: &str, item: &Value) -> String {
    let needle = format!("{{{{ {binding} }}}}");
    let replacement = match item {
        Value::String(value) => value.clone(),
        _ => item.to_string(),
    };
    template.replace(&needle, &replacement)
}

fn collect_dependencies(agent: &AgentDefinition) -> HashSet<String> {
    let mut dependencies = HashSet::new();

    if let Some(context) = &agent.context {
        let reference = match context {
            ContextSource::Full(reference) | ContextSource::Summary(reference) => reference,
        };
        if !reference.segments.is_empty() {
            let agent_name = if reference.segments[0] == "agent" {
                reference.segments.get(1)
            } else {
                Some(&reference.segments[0])
            };
            if let Some(name) = agent_name {
                dependencies.insert(name.clone());
            }
        }
    }

    if let Some(prompt) = &agent.prompt {
        collect_expression_dependencies(prompt, &mut dependencies);
    }

    if let Some(binding) = &agent.for_each {
        collect_expression_dependencies(&binding.collection, &mut dependencies);
    }

    dependencies
}

fn collect_expression_dependencies(expression: &Expression, dependencies: &mut HashSet<String>) {
    match expression {
        Expression::Array(items) => {
            for item in items {
                collect_expression_dependencies(item, dependencies);
            }
        }
        Expression::Object(values) => {
            for value in values.values() {
                collect_expression_dependencies(value, dependencies);
            }
        }
        Expression::Reference(reference) => {
            if reference.segments.first().map(String::as_str) != Some("schema") {
                let agent_name = if reference.segments.first().map(String::as_str) == Some("agent") {
                    reference.segments.get(1)
                } else {
                    reference.segments.first()
                };
                if let Some(name) = agent_name {
                    dependencies.insert(name.clone());
                }
            }
        }
        Expression::FunctionCall(function_call) => {
            collect_expression_dependencies(&function_call.target, dependencies);
            for value in function_call.arguments.values() {
                collect_expression_dependencies(value, dependencies);
            }
        }
        Expression::ForEach(binding) => collect_expression_dependencies(&binding.collection, dependencies),
        Expression::String(s) | Expression::MultilineString(s) | Expression::InterpolatedString(s) => {
            extract_template_dependencies(s, dependencies);
        }
        Expression::InlineSchema(_)
        | Expression::Number(_)
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::Identifier(_) => {}
    }
}

fn extract_template_dependencies(template: &str, dependencies: &mut HashSet<String>) {
    if let Ok(re) = regex::Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_.]*)\s*\}\}") {
        for cap in re.captures_iter(template) {
            if let Some(var_path) = cap.get(1) {
                let path = var_path.as_str();
                let parts: Vec<&str> = path.split('.').collect();
                if !parts.is_empty() && parts[0] != "schema" {
                    dependencies.insert(parts[0].to_string());
                }
            }
        }
    }
}

fn collect_terminal_outputs(
    document: &WorkflowDocument,
    results: &HashMap<String, AgentExecutionResult>,
) -> Result<Value, ExecutionError> {
    let terminals = document
        .agents
        .iter()
        .filter(|agent| agent.is_terminal)
        .collect::<Vec<_>>();

    if terminals.is_empty() {
        return Ok(Value::Null);
    }

    let mut output = Map::new();
    for agent in terminals {
        let result = results
            .get(&agent.name)
            .ok_or_else(|| ExecutionError::MissingAgentResult {
                agent: agent.name.clone(),
            })?;
        output.insert(agent.name.clone(), result.output.clone());
    }

    Ok(Value::Object(output))
}
