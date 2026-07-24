use lsp_types::DiagnosticSeverity;
use superwire_dsl::diagnostic::{
    Diagnostic as CoreDiagnostic, DiagnosticCode as CoreDiagnosticCode, DiagnosticSeverity as CoreDiagnosticSeverity,
};
use superwire_dsl::DslParseError;
use superwire_mcp::McpLock;
use superwire_semantic::build_dynamic_typed_workflow_ir;

use crate::diagnostic_code::DiagnosticCode;

use super::position::LineIndex;
use super::semantic_index::SemanticIndex;
use super::workflow_document::WorkflowDocument;
use super::{DocumentDiagnostic, DocumentDiagnosticRelated};

#[derive(Debug)]
pub struct SemanticSnapshot {
    workflow_document: WorkflowDocument,
    diagnostics: Vec<CoreDiagnostic>,
    pub semantic_index: SemanticIndex,
}

impl SemanticSnapshot {
    pub fn from_text(source_text: &str, mcp_lock: Option<&McpLock>) -> Self {
        let workflow_document = WorkflowDocument::from_source_with_mcp_lock(source_text, mcp_lock.cloned());

        match workflow_document.parse_result() {
            Ok(workflow) => {
                let semantic_index = SemanticIndex::from_workflow_document(&workflow_document);
                let mut diagnostics = workflow_document
                    .validation_report()
                    .map(superwire_dsl::ValidationReport::diagnostics)
                    .unwrap_or_default();

                if diagnostics.is_empty() && workflow.find_output().is_some() {
                    if let Err(semantic_error) = build_dynamic_typed_workflow_ir(workflow) {
                        diagnostics.push(semantic_error.diagnostic());
                    }
                }

                Self {
                    workflow_document,
                    diagnostics,
                    semantic_index,
                }
            }
            Err(parse_error) => {
                let diagnostics = vec![parse_error.diagnostic()];
                let mut semantic_index = SemanticIndex::from_text_fallback(source_text);
                semantic_index.mcp_lock = mcp_lock.cloned();

                Self {
                    workflow_document,
                    diagnostics,
                    semantic_index,
                }
            }
        }
    }

    pub fn parse_error(&self) -> Option<&DslParseError> {
        self.workflow_document.parse_error()
    }

    pub fn workflow_document(&self) -> &WorkflowDocument {
        &self.workflow_document
    }

    pub fn diagnostics(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentDiagnostic> {
        self.diagnostics
            .iter()
            .map(|core_diagnostic| document_diagnostic_from_core(core_diagnostic, source_text, line_index))
            .collect()
    }
}

fn document_diagnostic_from_core(core_diagnostic: &CoreDiagnostic, source_text: &str, line_index: &LineIndex) -> DocumentDiagnostic {
    let range = core_diagnostic.primary_span.map_or_else(
        || line_index.zero_range(),
        |source_span| line_index.source_span_range(source_text, source_span),
    );
    let related = core_diagnostic
        .secondary_labels
        .iter()
        .map(|secondary_label| DocumentDiagnosticRelated {
            range: line_index.source_span_range(source_text, secondary_label.span),
            message: secondary_label.message.clone().unwrap_or_else(|| core_diagnostic.message.clone()),
        })
        .collect();

    DocumentDiagnostic {
        range,
        severity: diagnostic_severity_from_core(core_diagnostic.severity),
        code: DiagnosticCode::from(core_diagnostic.code),
        message: core_diagnostic.message.clone(),
        related,
        notes: core_diagnostic.notes.clone(),
        help: core_diagnostic.help.clone(),
    }
}

fn diagnostic_severity_from_core(core_severity: CoreDiagnosticSeverity) -> DiagnosticSeverity {
    match core_severity {
        CoreDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        CoreDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        CoreDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        CoreDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

impl From<CoreDiagnosticCode> for DiagnosticCode {
    fn from(core_code: CoreDiagnosticCode) -> Self {
        match core_code {
            CoreDiagnosticCode::ParseError => Self::ParseError,
            CoreDiagnosticCode::MissingNode => Self::MissingNode,
            CoreDiagnosticCode::UnexpectedRule => Self::UnexpectedRule,
            CoreDiagnosticCode::InvalidIntegerLiteral => Self::InvalidIntegerLiteral,
            CoreDiagnosticCode::DuplicateProvider => Self::DuplicateProvider,
            CoreDiagnosticCode::InvalidProviderName => Self::InvalidProviderName,
            CoreDiagnosticCode::UnknownProviderDriver => Self::UnknownProviderDriver,
            CoreDiagnosticCode::DuplicateModel => Self::DuplicateModel,
            CoreDiagnosticCode::InvalidModelName => Self::InvalidModelName,
            CoreDiagnosticCode::UnknownProviderInModelDeclaration => Self::UnknownProviderInModelDeclaration,
            CoreDiagnosticCode::MissingModelId => Self::MissingModelId,
            CoreDiagnosticCode::UnknownModelProfile => Self::UnknownModelProfile,
            CoreDiagnosticCode::InvalidModelUsageProperty => Self::InvalidModelUsageProperty,
            CoreDiagnosticCode::DuplicateSchema => Self::DuplicateSchema,
            CoreDiagnosticCode::InvalidSchemaName => Self::InvalidSchemaName,
            CoreDiagnosticCode::InvalidVariantDiscriminatorField => Self::InvalidVariantDiscriminatorField,
            CoreDiagnosticCode::DuplicateTool => Self::DuplicateTool,
            CoreDiagnosticCode::DuplicateResource => Self::DuplicateResource,
            CoreDiagnosticCode::DuplicatePrompt => Self::DuplicatePrompt,
            CoreDiagnosticCode::DuplicateAgent => Self::DuplicateAgent,
            CoreDiagnosticCode::DuplicateSingletonDeclaration => Self::DuplicateSingletonDeclaration,
            CoreDiagnosticCode::DuplicateProperty => Self::DuplicateProperty,
            CoreDiagnosticCode::UnknownAgentProperty => Self::UnknownAgentProperty,
            CoreDiagnosticCode::UnsupportedAgentContextProperty => Self::UnsupportedAgentContextProperty,
            CoreDiagnosticCode::UnsupportedAgentFileProperty => Self::UnsupportedAgentFileProperty,
            CoreDiagnosticCode::MissingAgentFileContent => Self::MissingAgentFileContent,
            CoreDiagnosticCode::InvalidAgentFileWireApi => Self::InvalidAgentFileWireApi,
            CoreDiagnosticCode::InvalidInferenceSettingValueType => Self::InvalidInferenceSettingValueType,
            CoreDiagnosticCode::InvalidModelExpression => Self::InvalidModelExpression,
            CoreDiagnosticCode::UnknownProviderInModel => Self::UnknownProviderInModel,
            CoreDiagnosticCode::UnknownModelForProvider => Self::UnknownModelForProvider,
            CoreDiagnosticCode::UnknownAgentReference => Self::UnknownAgentReference,
            CoreDiagnosticCode::InvalidKeywordReferenceRoot => Self::InvalidKeywordReferenceRoot,
            CoreDiagnosticCode::MissingDynamicDeclaration => Self::MissingDynamicDeclaration,
            CoreDiagnosticCode::MissingInputDeclaration => Self::MissingInputDeclaration,
            CoreDiagnosticCode::MissingSecretsDeclaration => Self::MissingSecretsDeclaration,
            CoreDiagnosticCode::UnknownDynamicFieldReference => Self::UnknownDynamicFieldReference,
            CoreDiagnosticCode::UnknownLocalBindingReference => Self::UnknownLocalBindingReference,
            CoreDiagnosticCode::UnknownInputFieldReference => Self::UnknownInputFieldReference,
            CoreDiagnosticCode::UnknownSecretsFieldReference => Self::UnknownSecretsFieldReference,
            CoreDiagnosticCode::SecretReferenceInLlmContext => Self::SecretReferenceInLlmContext,
            CoreDiagnosticCode::MissingAgentOutputTypeForFieldReference => Self::MissingAgentOutputTypeForFieldReference,
            CoreDiagnosticCode::MissingOptionalReferenceAccess => Self::MissingOptionalReferenceAccess,
            CoreDiagnosticCode::InvalidReferencePath => Self::InvalidReferencePath,
            CoreDiagnosticCode::InvalidForLoopIterableType => Self::InvalidForLoopIterableType,
            CoreDiagnosticCode::InvalidForLoopDestructuringBinding => Self::InvalidForLoopDestructuringBinding,
            CoreDiagnosticCode::UnknownSchemaReference => Self::UnknownSchemaReference,
            CoreDiagnosticCode::UnknownToolReference => Self::UnknownToolReference,
            CoreDiagnosticCode::UnknownResourceReference => Self::UnknownResourceReference,
            CoreDiagnosticCode::UnknownPromptReference => Self::UnknownPromptReference,
            CoreDiagnosticCode::InvalidMcpToolSchema => Self::InvalidMcpToolSchema,
            CoreDiagnosticCode::InvalidToolBinding => Self::InvalidToolBinding,
            CoreDiagnosticCode::InvalidTypeExpressionReference => Self::InvalidTypeExpressionReference,
            CoreDiagnosticCode::AgentDependencyCycle => Self::AgentDependencyCycle,
            CoreDiagnosticCode::DynamicDependencyCycle => Self::DynamicDependencyCycle,
            CoreDiagnosticCode::WorkflowCompilationError => Self::WorkflowCompilationError,
        }
    }
}
