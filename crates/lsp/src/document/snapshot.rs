use engine_ai_core::diagnostic::{
    Diagnostic as CoreDiagnostic, DiagnosticCode as CoreDiagnosticCode, DiagnosticSeverity as CoreDiagnosticSeverity,
};
use engine_ai_core::dsl::{parse_workflow, validate_workflow, DslParseError, ValidationReport};

use crate::protocol::DiagnosticCode;

use super::position::{source_span_to_range, zero_range};
use super::semantic_index::SemanticIndex;
use super::{DiagnosticSeverity, DocumentDiagnostic};

#[derive(Debug)]
pub(super) struct SemanticSnapshot {
    pub(super) parse_error: Option<DslParseError>,
    validation_report: Option<ValidationReport>,
    pub(super) semantic_index: SemanticIndex,
}

impl SemanticSnapshot {
    pub(super) fn from_text(source_text: &str) -> Self {
        match parse_workflow(source_text) {
            Ok(workflow) => {
                let validation_report = validate_workflow(&workflow);
                let semantic_index = SemanticIndex::from_workflow(&workflow);

                Self {
                    parse_error: None,
                    validation_report: Some(validation_report),
                    semantic_index,
                }
            }
            Err(parse_error) => Self {
                parse_error: Some(parse_error),
                validation_report: None,
                semantic_index: SemanticIndex::from_text_fallback(source_text),
            },
        }
    }

    pub(super) fn diagnostics(&self, source_text: &str) -> Vec<DocumentDiagnostic> {
        if let Some(parse_error) = &self.parse_error {
            return vec![document_diagnostic_from_core(&parse_error.diagnostic(), source_text)];
        }

        let Some(validation_report) = &self.validation_report else {
            return Vec::new();
        };

        validation_report
            .diagnostics()
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
            CoreDiagnosticCode::DuplicateAgent => Self::DuplicateAgent,
            CoreDiagnosticCode::DuplicateSingletonDeclaration => Self::DuplicateSingletonDeclaration,
            CoreDiagnosticCode::UnknownAgentProperty => Self::UnknownAgentProperty,
            CoreDiagnosticCode::InvalidModelExpression => Self::InvalidModelExpression,
            CoreDiagnosticCode::UnknownProviderInModel => Self::UnknownProviderInModel,
            CoreDiagnosticCode::UnknownModelForProvider => Self::UnknownModelForProvider,
            CoreDiagnosticCode::UnknownAgentReference => Self::UnknownAgentReference,
            CoreDiagnosticCode::InvalidKeywordReferenceRoot => Self::InvalidKeywordReferenceRoot,
            CoreDiagnosticCode::MissingInputDeclaration => Self::MissingInputDeclaration,
            CoreDiagnosticCode::MissingSecretsDeclaration => Self::MissingSecretsDeclaration,
            CoreDiagnosticCode::UnknownInputFieldReference => Self::UnknownInputFieldReference,
            CoreDiagnosticCode::UnknownSecretsFieldReference => Self::UnknownSecretsFieldReference,
            CoreDiagnosticCode::SecretReferenceInLlmContext => Self::SecretReferenceInLlmContext,
            CoreDiagnosticCode::MissingAgentOutputTypeForFieldReference => Self::MissingAgentOutputTypeForFieldReference,
            CoreDiagnosticCode::InvalidReferencePath => Self::InvalidReferencePath,
            CoreDiagnosticCode::UnknownSchemaReference => Self::UnknownSchemaReference,
            CoreDiagnosticCode::AgentDependencyCycle => Self::AgentDependencyCycle,
        }
    }
}
