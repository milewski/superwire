use std::collections::{HashMap, HashSet};

use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;

pub mod error;

use crate::ast::{AgentDefinition, ContextSource, Expression, OutputDefinition, Reference, WorkflowDocument};
use crate::parser::graph::build_dependency_graph;
use error::ValidationError;

pub fn validate_workflow(document: &WorkflowDocument) -> Result<(), ValidationError> {
    validate_unique_names(document)?;
    validate_provider_configuration(document)?;
    validate_agent_properties(document)?;
    validate_references(document)?;
    validate_cycles(document)?;
    build_dependency_graph(document).map_err(|source| ValidationError::DependencyGraph {
        message: source.to_string(),
    })?;
    Ok(())
}

fn validate_unique_names(document: &WorkflowDocument) -> Result<(), ValidationError> {
    let mut agent_names = HashSet::new();
    for agent in &document.agents {
        if !agent_names.insert(agent.name.clone()) {
            return Err(ValidationError::DuplicateAgent {
                name: agent.name.clone(),
            });
        }
    }

    let mut schema_names = HashSet::new();
    for schema in &document.schemas {
        if let Some(name) = &schema.name {
            if !schema_names.insert(name.clone()) {
                return Err(ValidationError::DuplicateSchema { name: name.clone() });
            }
        }
    }

    let mut provider_names = HashSet::new();
    for provider in &document.providers {
        if !provider_names.insert(provider.name.clone()) {
            return Err(ValidationError::DuplicateProvider {
                name: provider.name.clone(),
            });
        }
    }

    let workflow_input_count = usize::from(document.input.is_some());
    if workflow_input_count > 1 {
        return Err(ValidationError::DuplicateWorkflowInput);
    }

    let workflow_output_count = usize::from(document.output.is_some());
    if workflow_output_count > 1 {
        return Err(ValidationError::DuplicateWorkflowOutput);
    }

    Ok(())
}

fn validate_provider_configuration(document: &WorkflowDocument) -> Result<(), ValidationError> {
    let providers = document
        .providers
        .iter()
        .map(|provider| (provider.name.as_str(), provider))
        .collect::<HashMap<_, _>>();

    for agent in &document.agents {
        if let Some(model) = &agent.model {
            let provider =
                providers
                    .get(model.provider.as_str())
                    .ok_or_else(|| ValidationError::UndefinedProvider {
                        agent: agent.name.clone(),
                        provider: model.provider.clone(),
                    })?;

            if !provider.models.iter().any(|candidate| candidate == &model.model) {
                return Err(ValidationError::ProviderModelMismatch {
                    agent: agent.name.clone(),
                    provider: model.provider.clone(),
                    model: model.model.clone(),
                });
            }
        }
    }

    Ok(())
}

fn validate_agent_properties(document: &WorkflowDocument) -> Result<(), ValidationError> {
    for agent in &document.agents {
        for property in agent.properties.keys() {
            match property.as_str() {
                "model" | "tools" | "context" | "output" | "prompt" | "for_each" => {}
                _ => {
                    return Err(ValidationError::InvalidProperty {
                        scope: format!("agent `{}`", agent.name),
                        property: property.clone(),
                    });
                }
            }
        }
    }

    for provider in &document.providers {
        for property in provider.properties.keys() {
            match property.as_str() {
                "driver" | "api_endpoint" | "models" => {}
                _ => {
                    return Err(ValidationError::InvalidProperty {
                        scope: format!("provider `{}`", provider.name),
                        property: property.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn validate_references(document: &WorkflowDocument) -> Result<(), ValidationError> {
    let agent_names = document
        .agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect::<HashSet<_>>();
    let schema_names = document
        .schemas
        .iter()
        .filter_map(|schema| schema.name.as_deref())
        .collect::<HashSet<_>>();

    for agent in &document.agents {
        if let Some(output) = &agent.output {
            match output {
                OutputDefinition::SchemaReference(name) if !schema_names.contains(name.as_str()) => {
                    return Err(ValidationError::UndefinedSchema {
                        agent: agent.name.clone(),
                        schema: name.clone(),
                    });
                }
                OutputDefinition::SchemaReference(_) | OutputDefinition::Inline(_) => {}
            }
        }

        if let Some(context) = &agent.context {
            let reference = match context {
                ContextSource::Full(reference) | ContextSource::Summary(reference) => reference,
            };
            validate_agent_reference(&format!("agent `{}`", agent.name), reference, &agent_names)?;
        }

        if let Some(prompt) = &agent.prompt {
            validate_expression_references(&format!("agent `{}`", agent.name), prompt, &agent_names, &schema_names)?;
        }

        if let Some(binding) = &agent.for_each {
            validate_expression_references(
                &format!("agent `{}`", agent.name),
                &binding.collection,
                &agent_names,
                &schema_names,
            )?;
        }
    }

    if let Some(output) = &document.output {
        validate_expression_references("workflow output", output, &agent_names, &schema_names)?;
    }

    Ok(())
}

fn validate_cycles(document: &WorkflowDocument) -> Result<(), ValidationError> {
    let mut graph = DiGraph::<&str, ()>::new();
    let mut nodes = HashMap::new();

    for agent in &document.agents {
        let index = graph.add_node(agent.name.as_str());
        nodes.insert(agent.name.as_str(), index);
    }

    for agent in &document.agents {
        let dependencies = collect_agent_dependencies(agent);
        for dependency in dependencies {
            if let Some(dep_index) = nodes.get(dependency.as_str()) {
                let agent_index = nodes[agent.name.as_str()];
                graph.add_edge(*dep_index, agent_index, ());
            }
        }
    }

    if is_cyclic_directed(&graph) {
        return Err(ValidationError::CyclicDependency);
    }

    Ok(())
}

fn validate_agent_reference(
    scope: &str,
    reference: &Reference,
    agent_names: &HashSet<&str>,
) -> Result<(), ValidationError> {
    if reference.segments.is_empty() {
        return Err(ValidationError::UndefinedAgent {
            scope: scope.to_string(),
            reference: reference.as_string(),
        });
    }

    let referenced_agent = if reference.segments[0] == "agent" {
        reference
            .segments
            .get(1)
            .ok_or_else(|| ValidationError::UndefinedAgent {
                scope: scope.to_string(),
                reference: reference.as_string(),
            })?
    } else {
        &reference.segments[0]
    };

    if !agent_names.contains(referenced_agent.as_str()) {
        return Err(ValidationError::UndefinedAgent {
            scope: scope.to_string(),
            reference: reference.as_string(),
        });
    }

    Ok(())
}

fn validate_expression_references(
    scope: &str,
    expression: &Expression,
    agent_names: &HashSet<&str>,
    schema_names: &HashSet<&str>,
) -> Result<(), ValidationError> {
    match expression {
        Expression::Array(items) => {
            for item in items {
                validate_expression_references(scope, item, agent_names, schema_names)?;
            }
        }
        Expression::Object(values) => {
            for value in values.values() {
                validate_expression_references(scope, value, agent_names, schema_names)?;
            }
        }
        Expression::Reference(reference) => match reference.segments.first().map(String::as_str) {
            Some("schema") => {
                if let Some(name) = reference.segments.get(1) {
                    if !schema_names.contains(name.as_str()) {
                        return Err(ValidationError::UndefinedSchema {
                            agent: scope.to_string(),
                            schema: name.clone(),
                        });
                    }
                }
            }
            Some("input") => {}
            _ => validate_agent_reference(scope, reference, agent_names)?,
        },
        Expression::FunctionCall(function_call) => {
            validate_expression_references(scope, &function_call.target, agent_names, schema_names)?;
            for value in function_call.arguments.values() {
                validate_expression_references(scope, value, agent_names, schema_names)?;
            }
        }
        Expression::InlineSchema(_)
        | Expression::ForEach(_)
        | Expression::String(_)
        | Expression::MultilineString(_)
        | Expression::Number(_)
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::Identifier(_)
        | Expression::InterpolatedString(_) => {}
    }

    Ok(())
}

fn collect_agent_dependencies(agent: &AgentDefinition) -> HashSet<String> {
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

    dependencies.remove(&agent.name);
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
            if matches!(
                reference.segments.first().map(String::as_str),
                Some("schema") | Some("input")
            ) {
                return;
            }

            let agent_name = if reference.segments.first().map(String::as_str) == Some("agent") {
                reference.segments.get(1)
            } else {
                reference.segments.first()
            };
            if let Some(name) = agent_name {
                dependencies.insert(name.clone());
            }
        }
        Expression::FunctionCall(function_call) => {
            collect_expression_dependencies(&function_call.target, dependencies);
            for value in function_call.arguments.values() {
                collect_expression_dependencies(value, dependencies);
            }
        }
        Expression::ForEach(binding) => collect_expression_dependencies(&binding.collection, dependencies),
        Expression::InlineSchema(_)
        | Expression::String(_)
        | Expression::MultilineString(_)
        | Expression::Number(_)
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::Identifier(_)
        | Expression::InterpolatedString(_) => {}
    }
}
