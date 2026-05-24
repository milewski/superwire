use super::ast::Workflow;
mod agents;
mod duplicates;
mod dynamic;
mod issues;
mod references;
mod schemas;
mod tools;

use agents::{validate_agent_inference_settings, validate_agent_model_bindings, validate_agent_tool_references};
use duplicates::WorkflowDuplicateValidationExt;
use dynamic::{validate_agent_dependency_cycles, validate_dynamic_dependency_cycles};
use references::validate_agent_references;
use schemas::validate_schema_references;

use superwire_semantic::WorkflowSemanticIndex;

pub use superwire_semantic::{SingletonDeclarationKind, ValidationContext, ValidationIssue, ValidationReport};

#[derive(Debug, Clone)]
pub struct WorkflowValidation {
    validation_report: ValidationReport,
    semantic_index: WorkflowSemanticIndex,
}

impl WorkflowValidation {
    #[must_use]
    pub fn validation_report(&self) -> &ValidationReport {
        &self.validation_report
    }

    #[must_use]
    pub fn semantic_index(&self) -> &WorkflowSemanticIndex {
        &self.semantic_index
    }

    #[must_use]
    pub fn into_validation_report(self) -> ValidationReport {
        self.validation_report
    }

    #[must_use]
    pub fn into_parts(self) -> (ValidationReport, WorkflowSemanticIndex) {
        (self.validation_report, self.semantic_index)
    }
}

pub trait WorkflowValidationExt {
    fn validate_with_semantic_index(&self) -> WorkflowValidation;
}

impl WorkflowValidationExt for Workflow {
    fn validate_with_semantic_index(&self) -> WorkflowValidation {
        let mut validation_report = ValidationReport::default();
        let semantic_index = WorkflowSemanticIndex::build_for_validation(self, &mut validation_report);

        self.validate_duplicate_properties(&mut validation_report);
        validate_schema_references(self, &semantic_index, &mut validation_report);
        validate_agent_inference_settings(self, &mut validation_report);
        validate_agent_model_bindings(self, &semantic_index, &mut validation_report);
        validate_agent_tool_references(self, &semantic_index, &mut validation_report);
        validate_agent_references(self, &semantic_index, &mut validation_report);
        validate_dynamic_dependency_cycles(self, &mut validation_report);
        validate_agent_dependency_cycles(self, &semantic_index, &mut validation_report);

        WorkflowValidation {
            validation_report,
            semantic_index,
        }
    }
}

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    workflow.validate_with_semantic_index().into_validation_report()
}

#[cfg(test)]
mod tests;
