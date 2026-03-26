use crate::dsl::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    ParseError,
    MissingNode,
    UnexpectedRule,
    InvalidIntegerLiteral,
    DuplicateProvider,
    DuplicateSchema,
    DuplicateAgent,
    DuplicateSingletonDeclaration,
    UnknownAgentProperty,
    InvalidInferenceSettingValueType,
    InvalidModelExpression,
    UnknownProviderInModel,
    UnknownModelForProvider,
    UnknownAgentReference,
    InvalidKeywordReferenceRoot,
    MissingInputDeclaration,
    MissingSecretsDeclaration,
    UnknownInputFieldReference,
    UnknownSecretsFieldReference,
    SecretReferenceInLlmContext,
    MissingAgentOutputTypeForFieldReference,
    InvalidReferencePath,
    UnknownSchemaReference,
    AgentDependencyCycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub span: SourceSpan,
    pub message: Option<String>,
}

impl DiagnosticLabel {
    #[must_use]
    pub fn new(span: SourceSpan) -> Self {
        Self { span, message: None }
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());

        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub primary_span: Option<SourceSpan>,
    pub secondary_labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: DiagnosticCode, severity: DiagnosticSeverity, message: impl Into<String>, primary_span: Option<SourceSpan>) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            primary_span,
            secondary_labels: Vec::new(),
            notes: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_secondary_label(mut self, secondary_label: DiagnosticLabel) -> Self {
        self.secondary_labels.push(secondary_label);

        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());

        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());

        self
    }
}
