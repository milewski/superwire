use superwire_core::diagnostic::{
    Diagnostic as CoreDiagnostic, DiagnosticCode as CoreDiagnosticCode, DiagnosticSeverity as CoreDiagnosticSeverity,
};
use superwire_core::dsl::{parse_workflow, validate_workflow, DslParseError};

use crate::protocol::DiagnosticCode;

use super::position::{source_span_to_range, zero_range};
use super::semantic_index::SemanticIndex;
use super::{DiagnosticSeverity, DocumentDiagnostic};

#[derive(Debug)]
pub struct SemanticSnapshot {
    pub parse_error: Option<DslParseError>,
    diagnostics: Vec<CoreDiagnostic>,
    pub semantic_index: SemanticIndex,
}

impl SemanticSnapshot {
    pub fn from_text(source_text: &str) -> Self {
        match parse_workflow(source_text) {
            Ok(workflow) => {
                let validation_report = validate_workflow(&workflow);
                let semantic_index = SemanticIndex::from_workflow(&workflow);
                let diagnostics = validation_report.diagnostics();

                Self {
                    parse_error: None,
                    diagnostics,
                    semantic_index,
                }
            }
            Err(parse_error) => {
                let diagnostics = vec![parse_error.diagnostic()];

                Self {
                    parse_error: Some(parse_error),
                    diagnostics,
                    semantic_index: SemanticIndex::from_text_fallback(source_text),
                }
            }
        }
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
        severity: DiagnosticSeverity::from(core_diagnostic.severity),
        code: DiagnosticCode::from(core_diagnostic.code),
        message: core_diagnostic.message.clone(),
    }
}

impl From<CoreDiagnosticSeverity> for DiagnosticSeverity {
    fn from(core_severity: CoreDiagnosticSeverity) -> Self {
        match core_severity {
            CoreDiagnosticSeverity::Error => Self::Error,
            CoreDiagnosticSeverity::Warning | CoreDiagnosticSeverity::Information | CoreDiagnosticSeverity::Hint => Self::Warning,
        }
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
            CoreDiagnosticCode::DuplicateSchema => Self::DuplicateSchema,
            CoreDiagnosticCode::DuplicateTool => Self::DuplicateTool,
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
            CoreDiagnosticCode::UnknownInputFieldReference => Self::UnknownInputFieldReference,
            CoreDiagnosticCode::UnknownSecretsFieldReference => Self::UnknownSecretsFieldReference,
            CoreDiagnosticCode::SecretReferenceInLlmContext => Self::SecretReferenceInLlmContext,
            CoreDiagnosticCode::MissingAgentOutputTypeForFieldReference => Self::MissingAgentOutputTypeForFieldReference,
            CoreDiagnosticCode::MissingOptionalReferenceAccess => Self::MissingOptionalReferenceAccess,
            CoreDiagnosticCode::InvalidReferencePath => Self::InvalidReferencePath,
            CoreDiagnosticCode::InvalidForLoopIterableType => Self::InvalidForLoopIterableType,
            CoreDiagnosticCode::UnknownSchemaReference => Self::UnknownSchemaReference,
            CoreDiagnosticCode::UnknownToolReference => Self::UnknownToolReference,
            CoreDiagnosticCode::InvalidTypeExpressionReference => Self::InvalidTypeExpressionReference,
            CoreDiagnosticCode::AgentDependencyCycle => Self::AgentDependencyCycle,
            CoreDiagnosticCode::DynamicDependencyCycle => Self::DynamicDependencyCycle,
            CoreDiagnosticCode::WorkflowCompilationError => Self::WorkflowCompilationError,
        }
    }
}
