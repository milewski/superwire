use super::ast::Workflow;
mod agents;
mod duplicates;
mod dynamic;
mod index;
mod names;
mod references;
mod report;
mod schemas;
mod tools;

use agents::{validate_agent_inference_settings, validate_agent_model_bindings, validate_agent_tool_references};
use dynamic::{validate_agent_dependency_cycles, validate_dynamic_dependency_cycles};
use index::ValidationIndex;
use references::validate_agent_references;
use schemas::validate_schema_references;

pub use report::{SingletonDeclarationKind, ValidationContext, ValidationIssue, ValidationReport};

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut validation_report = ValidationReport::default();
    let validation_index = ValidationIndex::build(workflow, &mut validation_report);

    workflow.validate_duplicate_properties(&mut validation_report);
    validate_schema_references(workflow, &validation_index, &mut validation_report);
    validate_agent_inference_settings(workflow, &mut validation_report);
    validate_agent_model_bindings(workflow, &validation_index, &mut validation_report);
    validate_agent_tool_references(workflow, &validation_index, &mut validation_report);
    validate_agent_references(workflow, &validation_index, &mut validation_report);
    validate_dynamic_dependency_cycles(workflow, &mut validation_report);
    validate_agent_dependency_cycles(workflow, &validation_index, &mut validation_report);

    validation_report
}

#[cfg(test)]
mod tests;
