use super::super::ast::{AgentProperty, Declaration, ObjectField, Workflow};
use super::index::ValidationIndex;
use super::report::{ValidationIssue, ValidationReport};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

pub(super) fn validate_dynamic_dependency_cycles(workflow: &Workflow, validation_report: &mut ValidationReport) {
    let workflow_dynamic_fields = workflow
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            Declaration::Dynamic(dynamic_block) => Some(dynamic_block.fields.as_slice()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();

    report_dynamic_dependency_cycles(workflow_dynamic_fields.as_slice(), validation_report);

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let agent_dynamic_fields = agent_declaration
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .collect::<Vec<_>>();

        report_dynamic_dependency_cycles(agent_dynamic_fields.as_slice(), validation_report);
    }
}

fn report_dynamic_dependency_cycles(dynamic_fields: &[&ObjectField], validation_report: &mut ValidationReport) {
    let mut dependency_graph = DiGraph::<String, ()>::new();
    let mut node_index_by_field_name = HashMap::<String, NodeIndex>::new();
    let mut sorted_field_names = dynamic_fields
        .iter()
        .map(|dynamic_field| dynamic_field.name.clone())
        .collect::<Vec<_>>();

    sorted_field_names.sort();

    for field_name in &sorted_field_names {
        let node_index = dependency_graph.add_node(field_name.clone());
        node_index_by_field_name.insert(field_name.clone(), node_index);
    }

    for dynamic_field in dynamic_fields {
        let Some(source_node) = node_index_by_field_name.get(&dynamic_field.name).copied() else {
            continue;
        };

        let mut referenced_dynamic_fields = HashSet::new();
        dynamic_field.value.collect_dynamic_dependencies(&mut referenced_dynamic_fields);

        for referenced_dynamic_field in referenced_dynamic_fields {
            let Some(target_node) = node_index_by_field_name.get(&referenced_dynamic_field).copied() else {
                continue;
            };

            if dependency_graph.find_edge(source_node, target_node).is_none() {
                dependency_graph.add_edge(source_node, target_node, ());
            }
        }
    }

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

        let mut cycle_field_names = strongly_connected_component
            .into_iter()
            .map(|node_index| dependency_graph[node_index].clone())
            .collect::<Vec<_>>();

        cycle_field_names.sort();

        validation_report.push_issue(ValidationIssue::DynamicDependencyCycle {
            field_names: cycle_field_names,
        });
    }
}

pub(super) fn validate_agent_dependency_cycles(
    workflow: &Workflow,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let mut dependency_graph = DiGraph::<String, ()>::new();
    let mut node_index_by_agent_name = HashMap::<String, NodeIndex>::new();
    let mut sorted_agent_names = validation_index.agent_names.iter().cloned().collect::<Vec<_>>();

    sorted_agent_names.sort();

    for agent_name in &sorted_agent_names {
        let node_index = dependency_graph.add_node(agent_name.clone());
        node_index_by_agent_name.insert(agent_name.clone(), node_index);
    }

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let Some(source_agent_node) = node_index_by_agent_name.get(&agent_declaration.name).copied() else {
            continue;
        };

        let mut referenced_agents = HashSet::new();

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            agent_for_loop.iterable.collect_agent_dependencies(&mut referenced_agents);
        }

        for agent_property in &agent_declaration.properties {
            match agent_property {
                AgentProperty::Dynamic(dynamic_block) => {
                    for dynamic_field in &dynamic_block.fields {
                        dynamic_field.value.collect_agent_dependencies(&mut referenced_agents);
                    }
                }
                AgentProperty::InvalidModel(model_expression)
                | AgentProperty::Instruction(model_expression)
                | AgentProperty::Context(model_expression)
                | AgentProperty::Uses(model_expression) => {
                    model_expression.collect_agent_dependencies(&mut referenced_agents);
                }
                AgentProperty::Model(model_usage) => {
                    for model_property in &model_usage.properties {
                        model_property.value.collect_agent_dependencies(&mut referenced_agents);
                    }
                }
                AgentProperty::Output { fields: _, span: _ } | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        for referenced_agent in referenced_agents {
            let Some(target_agent_node) = node_index_by_agent_name.get(&referenced_agent).copied() else {
                continue;
            };

            if dependency_graph.find_edge(source_agent_node, target_agent_node).is_none() {
                dependency_graph.add_edge(source_agent_node, target_agent_node, ());
            }
        }
    }

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

        let mut cycle_agent_names = strongly_connected_component
            .into_iter()
            .map(|node_index| dependency_graph[node_index].clone())
            .collect::<Vec<_>>();

        cycle_agent_names.sort();

        validation_report.push_issue(ValidationIssue::AgentDependencyCycle {
            agent_names: cycle_agent_names,
        });
    }
}
