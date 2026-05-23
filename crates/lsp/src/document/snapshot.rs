use lsp_types::DiagnosticSeverity;
use superwire_core::diagnostic::{
    Diagnostic as CoreDiagnostic, DiagnosticCode as CoreDiagnosticCode, DiagnosticSeverity as CoreDiagnosticSeverity,
};
use superwire_core::dsl::DslParseError;
use superwire_core::mcp::McpLock;
use superwire_core::semantic::build_dynamic_typed_workflow_ir;
use superwire_core::WorkflowDocument;

use crate::diagnostic_code::DiagnosticCode;

use super::position::{source_span_to_range, zero_range};
use super::semantic_index::SemanticIndex;
use super::DocumentDiagnostic;

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
                    .map(superwire_core::dsl::ValidationReport::diagnostics)
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

    pub fn diagnostics(&self, source_text: &str) -> Vec<DocumentDiagnostic> {
        self.diagnostics
            .iter()
            .map(|core_diagnostic| document_diagnostic_from_core(core_diagnostic, source_text))
            .collect()
    }
}

fn document_diagnostic_from_core(core_diagnostic: &CoreDiagnostic, source_text: &str) -> DocumentDiagnostic {
    let range = core_diagnostic
        .primary_span
        .map_or_else(zero_range, |source_span| source_span_to_range(source_text, source_span));

    DocumentDiagnostic {
        range,
        severity: diagnostic_severity_from_core(core_diagnostic.severity),
        code: DiagnosticCode::from(core_diagnostic.code),
        message: core_diagnostic.message.clone(),
    }
}

fn diagnostic_severity_from_core(core_severity: CoreDiagnosticSeverity) -> DiagnosticSeverity {
    match core_severity {
        CoreDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        CoreDiagnosticSeverity::Warning | CoreDiagnosticSeverity::Information | CoreDiagnosticSeverity::Hint => DiagnosticSeverity::WARNING,
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
            CoreDiagnosticCode::InvalidToolBinding => Self::InvalidToolBinding,
            CoreDiagnosticCode::InvalidTypeExpressionReference => Self::InvalidTypeExpressionReference,
            CoreDiagnosticCode::AgentDependencyCycle => Self::AgentDependencyCycle,
            CoreDiagnosticCode::DynamicDependencyCycle => Self::DynamicDependencyCycle,
            CoreDiagnosticCode::WorkflowCompilationError => Self::WorkflowCompilationError,
        }
    }
}
