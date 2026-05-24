use crate::dsl::{parse_workflow, DslParseError, ValidationReport, Workflow, WorkflowValidationExt};
use crate::mcp::McpLock;
use crate::semantic::WorkflowSemanticIndex;

#[derive(Debug)]
pub struct WorkflowDocument {
    source_text: String,
    parse_result: WorkflowDocumentParseResult,
    validation_report: Option<ValidationReport>,
    semantic_index: Option<WorkflowSemanticIndex>,
    mcp_enrichment: Option<WorkflowDocumentMcpEnrichment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDocumentMcpEnrichment {
    mcp_lock: McpLock,
}

#[derive(Debug)]
enum WorkflowDocumentParseResult {
    Parsed(Workflow),
    Failed(DslParseError),
}

impl WorkflowDocument {
    #[must_use]
    pub fn from_source(source_text: impl Into<String>) -> Self {
        Self::from_source_with_mcp_lock(source_text, None)
    }

    #[must_use]
    pub fn from_source_with_mcp_lock(source_text: impl Into<String>, mcp_lock: Option<McpLock>) -> Self {
        let source_text = source_text.into();
        let mcp_enrichment = mcp_lock.map(WorkflowDocumentMcpEnrichment::new);

        match parse_workflow(&source_text) {
            Ok(mut workflow) => {
                if let Some(mcp_enrichment) = &mcp_enrichment {
                    mcp_enrichment.apply_to_workflow(&mut workflow);
                }

                let workflow_validation = workflow.validate_with_semantic_index();
                let (validation_report, semantic_index) = workflow_validation.into_parts();

                Self {
                    source_text,
                    parse_result: WorkflowDocumentParseResult::Parsed(workflow),
                    validation_report: Some(validation_report),
                    semantic_index: Some(semantic_index),
                    mcp_enrichment,
                }
            }
            Err(parse_error) => Self {
                source_text,
                parse_result: WorkflowDocumentParseResult::Failed(parse_error),
                validation_report: None,
                semantic_index: None,
                mcp_enrichment,
            },
        }
    }

    #[must_use]
    pub fn source_text(&self) -> &str {
        &self.source_text
    }

    #[must_use]
    pub fn workflow(&self) -> Option<&Workflow> {
        match &self.parse_result {
            WorkflowDocumentParseResult::Parsed(workflow) => Some(workflow),
            WorkflowDocumentParseResult::Failed(_) => None,
        }
    }

    #[must_use]
    pub fn parse_error(&self) -> Option<&DslParseError> {
        match &self.parse_result {
            WorkflowDocumentParseResult::Parsed(_) => None,
            WorkflowDocumentParseResult::Failed(parse_error) => Some(parse_error),
        }
    }

    pub fn parse_result(&self) -> Result<&Workflow, &DslParseError> {
        match &self.parse_result {
            WorkflowDocumentParseResult::Parsed(workflow) => Ok(workflow),
            WorkflowDocumentParseResult::Failed(parse_error) => Err(parse_error),
        }
    }

    #[must_use]
    pub fn validation_report(&self) -> Option<&ValidationReport> {
        self.validation_report.as_ref()
    }

    #[must_use]
    pub fn semantic_index(&self) -> Option<&WorkflowSemanticIndex> {
        self.semantic_index.as_ref()
    }

    #[must_use]
    pub fn mcp_enrichment(&self) -> Option<&WorkflowDocumentMcpEnrichment> {
        self.mcp_enrichment.as_ref()
    }

    #[must_use]
    pub fn mcp_lock(&self) -> Option<&McpLock> {
        self.mcp_enrichment.as_ref().map(WorkflowDocumentMcpEnrichment::mcp_lock)
    }
}

impl WorkflowDocumentMcpEnrichment {
    #[must_use]
    pub fn new(mcp_lock: McpLock) -> Self {
        Self { mcp_lock }
    }

    #[must_use]
    pub fn mcp_lock(&self) -> &McpLock {
        &self.mcp_lock
    }

    fn apply_to_workflow(&self, workflow: &mut Workflow) {
        self.mcp_lock.apply_to_workflow(workflow);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::document::WorkflowDocument;
    use crate::mcp::{McpLock, McpServerLock};

    #[test]
    fn workflow_document_caches_parsed_validation_and_semantic_outputs() {
        let source_text = crate::workflow_source! {
            input {
                request: string
            }

            output {
                request: input.request
            }
        };
        let workflow_document = WorkflowDocument::from_source(source_text);

        assert!(workflow_document.parse_result().is_ok());
        assert!(workflow_document
            .validation_report()
            .is_some_and(|validation_report| !validation_report.has_issues()));
        assert!(workflow_document
            .semantic_index()
            .is_some_and(|semantic_index| semantic_index.input_type().is_some()));
    }

    #[test]
    fn workflow_document_keeps_optional_mcp_enrichment() {
        let source_text = crate::workflow_source! {
            from mcp.local {}

            output {
                done: true
            }
        };
        let mcp_lock = McpLock {
            servers: BTreeMap::from([("local".to_string(), McpServerLock::default())]),
        };
        let workflow_document = WorkflowDocument::from_source_with_mcp_lock(source_text, Some(mcp_lock));

        assert!(workflow_document.mcp_lock().is_some());
        assert!(workflow_document.workflow().is_some());
    }
}
