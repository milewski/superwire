use crate::ast::SourceSpan;
use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use std::io::IsTerminal;

#[must_use]
pub fn should_render_rich_diagnostics() -> bool {
    if std::env::var("SUPERWIRE_ERROR_FORMAT").ok().as_deref() == Some("json") {
        return false;
    }

    std::io::stderr().is_terminal()
}

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
    InvalidProviderName,
    UnknownProviderDriver,
    DuplicateModel,
    InvalidModelName,
    UnknownProviderInModelDeclaration,
    MissingModelId,
    UnknownModelProfile,
    InvalidModelUsageProperty,
    DuplicateSchema,
    InvalidSchemaName,
    InvalidVariantDiscriminatorField,
    DuplicateTool,
    DuplicateResource,
    DuplicatePrompt,
    DuplicateAgent,
    DuplicateSingletonDeclaration,
    DuplicateProperty,
    UnknownAgentProperty,
    UnsupportedAgentContextProperty,
    InvalidInferenceSettingValueType,
    InvalidModelExpression,
    UnknownProviderInModel,
    UnknownModelForProvider,
    UnknownAgentReference,
    InvalidKeywordReferenceRoot,
    MissingDynamicDeclaration,
    MissingInputDeclaration,
    MissingSecretsDeclaration,
    UnknownDynamicFieldReference,
    UnknownLocalBindingReference,
    UnknownInputFieldReference,
    UnknownSecretsFieldReference,
    SecretReferenceInLlmContext,
    MissingAgentOutputTypeForFieldReference,
    MissingOptionalReferenceAccess,
    InvalidReferencePath,
    InvalidForLoopIterableType,
    InvalidForLoopDestructuringBinding,
    UnknownSchemaReference,
    UnknownToolReference,
    UnknownResourceReference,
    UnknownPromptReference,
    InvalidToolBinding,
    InvalidTypeExpressionReference,
    AgentDependencyCycle,
    DynamicDependencyCycle,
    WorkflowCompilationError,
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
            Self::InvalidProviderName => "invalid_provider_name",
            Self::UnknownProviderDriver => "unknown_provider_driver",
            Self::DuplicateModel => "duplicate_model",
            Self::InvalidModelName => "invalid_model_name",
            Self::UnknownProviderInModelDeclaration => "unknown_provider_in_model_declaration",
            Self::MissingModelId => "missing_model_id",
            Self::UnknownModelProfile => "unknown_model_profile",
            Self::InvalidModelUsageProperty => "invalid_model_usage_property",
            Self::DuplicateSchema => "duplicate_schema",
            Self::InvalidSchemaName => "invalid_schema_name",
            Self::InvalidVariantDiscriminatorField => "invalid_variant_discriminator_field",
            Self::DuplicateTool => "duplicate_tool",
            Self::DuplicateResource => "duplicate_resource",
            Self::DuplicatePrompt => "duplicate_prompt",
            Self::DuplicateAgent => "duplicate_agent",
            Self::DuplicateSingletonDeclaration => "duplicate_singleton_declaration",
            Self::DuplicateProperty => "duplicate_property",
            Self::UnknownAgentProperty => "unknown_agent_property",
            Self::UnsupportedAgentContextProperty => "unsupported_agent_context_property",
            Self::InvalidInferenceSettingValueType => "invalid_inference_setting_value_type",
            Self::InvalidModelExpression => "invalid_model_expression",
            Self::UnknownProviderInModel => "unknown_provider_in_model",
            Self::UnknownModelForProvider => "unknown_model_for_provider",
            Self::UnknownAgentReference => "unknown_agent_reference",
            Self::InvalidKeywordReferenceRoot => "invalid_keyword_reference_root",
            Self::MissingDynamicDeclaration => "missing_dynamic_declaration",
            Self::MissingInputDeclaration => "missing_input_declaration",
            Self::MissingSecretsDeclaration => "missing_secrets_declaration",
            Self::UnknownDynamicFieldReference => "unknown_dynamic_field_reference",
            Self::UnknownLocalBindingReference => "unknown_local_binding_reference",
            Self::UnknownInputFieldReference => "unknown_input_field_reference",
            Self::UnknownSecretsFieldReference => "unknown_secrets_field_reference",
            Self::SecretReferenceInLlmContext => "secret_reference_in_llm_context",
            Self::MissingAgentOutputTypeForFieldReference => "missing_agent_output_type_for_field_reference",
            Self::MissingOptionalReferenceAccess => "missing_optional_reference_access",
            Self::InvalidReferencePath => "invalid_reference_path",
            Self::InvalidForLoopIterableType => "invalid_for_loop_iterable_type",
            Self::InvalidForLoopDestructuringBinding => "invalid_for_loop_destructuring_binding",
            Self::UnknownSchemaReference => "unknown_schema_reference",
            Self::UnknownToolReference => "unknown_tool_reference",
            Self::UnknownResourceReference => "unknown_resource_reference",
            Self::UnknownPromptReference => "unknown_prompt_reference",
            Self::InvalidToolBinding => "invalid_tool_binding",
            Self::InvalidTypeExpressionReference => "invalid_type_expression_reference",
            Self::AgentDependencyCycle => "agent_dependency_cycle",
            Self::DynamicDependencyCycle => "dynamic_dependency_cycle",
            Self::WorkflowCompilationError => "workflow_compilation_error",
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
    pub fn render(&self) -> String {
        self.render_without_source()
    }

    #[must_use]
    pub fn render_with_source(&self, source_text: &str, source_name: &str) -> String {
        let Some(primary_span) = self.primary_span else {
            return self.render_without_source();
        };

        let Some(primary_label_span) = primary_span.to_byte_range(source_text) else {
            return self.render_without_source();
        };

        let report_kind = match self.severity {
            DiagnosticSeverity::Warning => ReportKind::Warning,
            DiagnosticSeverity::Error | DiagnosticSeverity::Information | DiagnosticSeverity::Hint => ReportKind::Error,
        };

        let mut report_builder = Report::build(report_kind, (source_name, primary_label_span.clone()))
            .with_config(Config::default().with_color(false))
            .with_code(self.code.as_str())
            .with_message(self.message.clone())
            .with_label(
                Label::new((source_name, primary_label_span))
                    .with_color(Color::Red)
                    .with_message("here"),
            );

        for secondary_label in &self.secondary_labels {
            let Some(secondary_label_span) = secondary_label.span.to_byte_range(source_text) else {
                continue;
            };

            let mut report_label = Label::new((source_name, secondary_label_span)).with_color(Color::Yellow);

            if let Some(label_message) = &secondary_label.message {
                report_label = report_label.with_message(label_message.clone());
            }

            report_builder = report_builder.with_label(report_label);
        }

        for note in &self.notes {
            report_builder = report_builder.with_note(note.clone());
        }

        if let Some(help_message) = &self.help {
            report_builder = report_builder.with_help(help_message.clone());
        }

        let report = report_builder.finish();
        let mut rendered_output = Vec::new();

        if report
            .write((source_name, Source::from(source_text)), &mut rendered_output)
            .is_err()
        {
            return self.render_without_source();
        }

        String::from_utf8(rendered_output).unwrap_or_else(|_| self.render_without_source())
    }

    #[must_use]
    pub fn render_without_source(&self) -> String {
        let mut rendered_message = if let Some(primary_span) = self.primary_span {
            format!(
                "{}[{}]: {} at {}:{}",
                self.severity.as_str(),
                self.code.as_str(),
                self.message,
                primary_span.start.line,
                primary_span.start.column
            )
        } else {
            format!("{}[{}]: {}", self.severity.as_str(), self.code.as_str(), self.message)
        };

        for note in &self.notes {
            rendered_message.push_str("\nnote: ");
            rendered_message.push_str(note);
        }

        if let Some(help_message) = &self.help {
            rendered_message.push_str("\nhelp: ");
            rendered_message.push_str(help_message);
        }

        rendered_message
    }
}
