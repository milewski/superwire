use engine_ai_core::dsl::{parse_workflow, validate_workflow, DslParseError, ValidationIssue, ValidationReport};

use crate::protocol::DiagnosticCode;

use super::semantic_index::SemanticIndex;
use super::{source_span_to_range, zero_range, DiagnosticSeverity, DocumentDiagnostic};

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
        if self.parse_error.is_some() {
            return self.parse_diagnostics(source_text);
        }

        let Some(validation_report) = &self.validation_report else {
            return Vec::new();
        };

        validation_report
            .issues_with_spans()
            .map(|(validation_issue, optional_span)| {
                let range = optional_span.map_or_else(zero_range, |source_span| source_span_to_range(source_text, source_span));

                DocumentDiagnostic {
                    range,
                    severity: DiagnosticSeverity::Error,
                    code: DiagnosticCode::from(validation_issue),
                    message: validation_issue.message(),
                }
            })
            .collect()
    }

    fn parse_diagnostics(&self, source_text: &str) -> Vec<DocumentDiagnostic> {
        let Some(parse_error) = &self.parse_error else {
            return Vec::new();
        };

        let range = parse_error
            .span()
            .map_or_else(zero_range, |source_span| source_span_to_range(source_text, source_span));

        vec![DocumentDiagnostic {
            range,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::from(parse_error),
            message: parse_error.to_string(),
        }]
    }
}

impl From<&DslParseError> for DiagnosticCode {
    fn from(parse_error: &DslParseError) -> Self {
        match parse_error {
            DslParseError::Pest { message: _, span: _ } => Self::ParseError,
            DslParseError::MissingNode {
                expected: _,
                context: _,
                span: _,
            } => Self::MissingNode,
            DslParseError::UnexpectedRule {
                rule: _,
                context: _,
                span: _,
            } => Self::UnexpectedRule,
            DslParseError::InvalidIntegerLiteral {
                literal: _,
                context: _,
                span: _,
            } => Self::InvalidIntegerLiteral,
        }
    }
}

impl From<&ValidationIssue> for DiagnosticCode {
    fn from(validation_issue: &ValidationIssue) -> Self {
        match validation_issue {
            ValidationIssue::DuplicateProvider { provider_name: _ } => Self::DuplicateProvider,
            ValidationIssue::DuplicateSchema { schema_name: _ } => Self::DuplicateSchema,
            ValidationIssue::DuplicateAgent { agent_name: _ } => Self::DuplicateAgent,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind: _ } => Self::DuplicateSingletonDeclaration,
            ValidationIssue::UnknownAgentProperty {
                agent_name: _,
                property_name: _,
            } => Self::UnknownAgentProperty,
            ValidationIssue::InvalidModelExpression { agent_name: _ } => Self::InvalidModelExpression,
            ValidationIssue::UnknownProviderInModel {
                agent_name: _,
                provider_name: _,
            } => Self::UnknownProviderInModel,
            ValidationIssue::UnknownModelForProvider {
                agent_name: _,
                provider_name: _,
                model_name: _,
            } => Self::UnknownModelForProvider,
            ValidationIssue::UnknownAgentReference {
                referenced_agent: _,
                context: _,
            } => Self::UnknownAgentReference,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword: _, context: _ } => Self::InvalidKeywordReferenceRoot,
            ValidationIssue::MissingInputDeclaration { context: _ } => Self::MissingInputDeclaration,
            ValidationIssue::MissingSecretsDeclaration { context: _ } => Self::MissingSecretsDeclaration,
            ValidationIssue::UnknownInputFieldReference { field_name: _, context: _ } => Self::UnknownInputFieldReference,
            ValidationIssue::UnknownSecretsFieldReference { field_name: _, context: _ } => Self::UnknownSecretsFieldReference,
            ValidationIssue::SecretReferenceInLlmContext {
                reference_path: _,
                context: _,
            } => Self::SecretReferenceInLlmContext,
            ValidationIssue::MissingAgentOutputTypeForFieldReference { agent_name: _, context: _ } => {
                Self::MissingAgentOutputTypeForFieldReference
            }
            ValidationIssue::InvalidReferencePath {
                reference_path: _,
                invalid_field: _,
                context: _,
            } => Self::InvalidReferencePath,
            ValidationIssue::UnknownSchemaReference {
                referenced_schema: _,
                context: _,
            } => Self::UnknownSchemaReference,
            ValidationIssue::AgentDependencyCycle { agent_names: _ } => Self::AgentDependencyCycle,
        }
    }
}
