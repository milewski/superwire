use crate::dsl::{AgentProperty, CallArgument, Declaration, Expression, Reference, ReferenceKeyword, StringTemplatePart, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

pub(crate) fn determine_agent_execution_order(workflow: &Workflow) -> Result<Vec<String>, WorkflowRuntimeError> {
    let mut dependency_graph = DiGraph::<String, ()>::new();
    let mut node_index_by_agent_name = HashMap::<String, NodeIndex>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let node_index = dependency_graph.add_node(agent_declaration.name.clone());
        node_index_by_agent_name.insert(agent_declaration.name.clone(), node_index);
    }

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let source_node_index = node_index_by_agent_name
            .get(agent_declaration.name.as_str())
            .copied()
            .expect("agent nodes should be inserted before dependency edges");
        let mut referenced_agent_names = HashSet::<String>::new();

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            collect_agent_dependencies_from_expression(&agent_for_loop.iterable, &mut referenced_agent_names);
        }

        for agent_property in &agent_declaration.properties {
            match agent_property {
                AgentProperty::Model(model_expression)
                | AgentProperty::Prompt(model_expression)
                | AgentProperty::Context(model_expression)
                | AgentProperty::Inference(model_expression)
                | AgentProperty::Tools(model_expression)
                | AgentProperty::Custom {
                    name: _,
                    value: model_expression,
                } => {
                    collect_agent_dependencies_from_expression(model_expression, &mut referenced_agent_names);
                }
                AgentProperty::Output(_) => {}
            }
        }

        for referenced_agent_name in referenced_agent_names {
            let Some(target_node_index) = node_index_by_agent_name.get(referenced_agent_name.as_str()).copied() else {
                continue;
            };

            if dependency_graph.find_edge(target_node_index, source_node_index).is_none() {
                dependency_graph.add_edge(target_node_index, source_node_index, ());
            }
        }
    }

    if toposort(&dependency_graph, None).is_err() {
        let mut cycle_agent_names = Vec::<String>::new();

        for strongly_connected_component in kosaraju_scc(&dependency_graph) {
            let has_cycle = if strongly_connected_component.len() > 1 {
                true
            } else {
                let node_index = strongly_connected_component[0];

                dependency_graph.find_edge(node_index, node_index).is_some()
            };

            if !has_cycle {
                continue;
            }

            for node_index in strongly_connected_component {
                cycle_agent_names.push(dependency_graph[node_index].clone());
            }
        }

        cycle_agent_names.sort();
        cycle_agent_names.dedup();

        return Err(WorkflowRuntimeError::DependencyCycle {
            agent_names: cycle_agent_names,
        });
    }

    let sorted_node_indices =
        toposort(&dependency_graph, None).expect("toposort should succeed after cycle pre-check in the same function");

    Ok(sorted_node_indices
        .into_iter()
        .map(|node_index| dependency_graph[node_index].clone())
        .collect())
}

fn collect_agent_dependencies_from_expression(expression: &Expression, referenced_agent_names: &mut HashSet<String>) {
    match expression {
        Expression::Reference(reference) => {
            collect_agent_dependency_from_reference(reference, referenced_agent_names);
        }
        Expression::FunctionCall(function_call) => {
            collect_agent_dependency_from_reference(&function_call.callee, referenced_agent_names);

            for call_argument in &function_call.arguments {
                match call_argument {
                    CallArgument::Positional(argument_expression) => {
                        collect_agent_dependencies_from_expression(argument_expression, referenced_agent_names);
                    }
                    CallArgument::Named(named_argument) => {
                        collect_agent_dependencies_from_expression(&named_argument.value, referenced_agent_names);
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_values) => {
            for array_value in array_values {
                collect_agent_dependencies_from_expression(array_value, referenced_agent_names);
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            for object_field in object_fields {
                collect_agent_dependencies_from_expression(&object_field.value, referenced_agent_names);
            }
        }
        Expression::StringTemplate(string_template) => {
            for string_template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                    collect_agent_dependencies_from_expression(interpolation_expression, referenced_agent_names);
                }
            }
        }
        Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
    }
}

fn collect_agent_dependency_from_reference(reference: &Reference, referenced_agent_names: &mut HashSet<String>) {
    if reference.root.keyword() != Some(ReferenceKeyword::Agent) {
        return;
    }

    let Some(first_access) = reference.accesses.first() else {
        return;
    };

    referenced_agent_names.insert(first_access.field.clone());
}
