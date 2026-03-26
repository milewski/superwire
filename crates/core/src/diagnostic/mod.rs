use crate::dsl::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl DiagnosticSeverity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Information => "information",
            Self::Hint => "hint",
        }
    }
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

impl DiagnosticCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::MissingNode => "missing_node",
            Self::UnexpectedRule => "unexpected_rule",
            Self::InvalidIntegerLiteral => "invalid_integer_literal",
            Self::DuplicateProvider => "duplicate_provider",
            Self::DuplicateSchema => "duplicate_schema",
            Self::DuplicateAgent => "duplicate_agent",
            Self::DuplicateSingletonDeclaration => "duplicate_singleton_declaration",
            Self::UnknownAgentProperty => "unknown_agent_property",
            Self::InvalidInferenceSettingValueType => "invalid_inference_setting_value_type",
            Self::InvalidModelExpression => "invalid_model_expression",
            Self::UnknownProviderInModel => "unknown_provider_in_model",
            Self::UnknownModelForProvider => "unknown_model_for_provider",
            Self::UnknownAgentReference => "unknown_agent_reference",
            Self::InvalidKeywordReferenceRoot => "invalid_keyword_reference_root",
            Self::MissingInputDeclaration => "missing_input_declaration",
            Self::MissingSecretsDeclaration => "missing_secrets_declaration",
            Self::UnknownInputFieldReference => "unknown_input_field_reference",
            Self::UnknownSecretsFieldReference => "unknown_secrets_field_reference",
            Self::SecretReferenceInLlmContext => "secret_reference_in_llm_context",
            Self::MissingAgentOutputTypeForFieldReference => "missing_agent_output_type_for_field_reference",
            Self::InvalidReferencePath => "invalid_reference_path",
            Self::UnknownSchemaReference => "unknown_schema_reference",
            Self::AgentDependencyCycle => "agent_dependency_cycle",
        }
    }
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

    #[must_use]
    pub fn render_for_cli(&self, source_path: Option<&str>) -> String {
        let mut rendered_lines = Vec::new();
        rendered_lines.push(format!("{}[{}]: {}", self.severity.as_str(), self.code.as_str(), self.message));

        if let Some(primary_span) = self.primary_span {
            rendered_lines.push(format!("  --> {}", primary_span.display_for_cli(source_path)));
        }

        for secondary_label in &self.secondary_labels {
            let message = secondary_label.message.as_deref().unwrap_or("additional context");
            rendered_lines.push(format!("  = {} at {}", message, secondary_label.span.display_for_cli(source_path)));
        }

        for note in &self.notes {
            rendered_lines.push(format!("  note: {note}"));
        }

        if let Some(help) = &self.help {
            rendered_lines.push(format!("  help: {help}"));
        }

        rendered_lines.join("\n")
    }
}

#[must_use]
pub fn render_diagnostics_for_cli(diagnostics: &[Diagnostic], source_path: Option<&str>) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render_for_cli(source_path))
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl SourceSpan {
    #[must_use]
    pub fn display_for_cli(self, source_path: Option<&str>) -> String {
        if let Some(source_path) = source_path {
            return format!("{source_path}:{}:{}", self.start.line, self.start.column);
        }

        format!("{}:{}", self.start.line, self.start.column)
    }
}

#[cfg(test)]
mod tests {
    use super::{render_diagnostics_for_cli, Diagnostic, DiagnosticCode, DiagnosticLabel, DiagnosticSeverity};
    use crate::dsl::{SourcePosition, SourceSpan};

    #[test]
    fn renders_cli_diagnostic_with_code_severity_and_span() {
        let source_span = SourceSpan {
            start: SourcePosition { line: 3, column: 9 },
            end: SourcePosition { line: 3, column: 14 },
        };

        let diagnostic = Diagnostic::new(
            DiagnosticCode::UnknownInputFieldReference,
            DiagnosticSeverity::Error,
            "Unknown input field `missing`.",
            Some(source_span),
        )
        .with_secondary_label(DiagnosticLabel::new(source_span).with_message("referenced here"))
        .with_note("declare the field in `input`")
        .with_help("add `missing: string` to `input`");

        let rendered = render_diagnostics_for_cli(&[diagnostic], Some("workflow.ai"));

        assert!(rendered.contains("error[unknown_input_field_reference]: Unknown input field `missing`."));
        assert!(rendered.contains("--> workflow.ai:3:9"));
        assert!(rendered.contains("= referenced here at workflow.ai:3:9"));
        assert!(rendered.contains("note: declare the field in `input`"));
        assert!(rendered.contains("help: add `missing: string` to `input`"));
    }
}
