use crate::dsl::{
    DslParseError, ReferenceKeyword, SingletonDeclarationKind, SourceSpan, ValidationContext, ValidationIssue, ValidationReport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    ParsePest,
    ParseMissingNode,
    ParseUnexpectedRule,
    ParseInvalidIntegerLiteral,
    ValidationDuplicateProvider,
    ValidationDuplicateSchema,
    ValidationDuplicateAgent,
    ValidationDuplicateSingleton,
    ValidationInvalidModelExpression,
    ValidationUnknownProviderInModel,
    ValidationUnknownModelForProvider,
    ValidationUnknownAgentReference,
    ValidationInvalidKeywordReferenceRoot,
    ValidationMissingInputDeclaration,
    ValidationMissingSecretsDeclaration,
    ValidationUnknownInputFieldReference,
    ValidationUnknownSecretsFieldReference,
    ValidationSecretReferenceLeak,
    ValidationMissingAgentOutputType,
    ValidationInvalidReferencePath,
    ValidationUnknownSchemaReference,
    ValidationAgentDependencyCycle,
    RuntimeInvalidWorkflow,
    RuntimeExecutionPlanInvariant,
    RuntimeMissingDeclaration,
    RuntimeUnsupportedFeature,
    RuntimeProviderConfiguration,
    RuntimeExpressionEvaluation,
    RuntimeInvalidAgentProperty,
    RuntimeInputTypeMismatch,
    RuntimeOutputTypeMismatch,
    RuntimeInputValueMismatch,
    RuntimeAgentOutputTypeMismatch,
    RuntimeAgentExecutionFailed,
    RuntimeSerializationFailed,
    RuntimeOutputDeserializationFailed,
    RuntimeOther,
}

impl DiagnosticCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParsePest => "WF1001",
            Self::ParseMissingNode => "WF1002",
            Self::ParseUnexpectedRule => "WF1003",
            Self::ParseInvalidIntegerLiteral => "WF1004",
            Self::ValidationDuplicateProvider => "WF2001",
            Self::ValidationDuplicateSchema => "WF2002",
            Self::ValidationDuplicateAgent => "WF2003",
            Self::ValidationDuplicateSingleton => "WF2004",
            Self::ValidationInvalidModelExpression => "WF2005",
            Self::ValidationUnknownProviderInModel => "WF2006",
            Self::ValidationUnknownModelForProvider => "WF2007",
            Self::ValidationUnknownAgentReference => "WF2008",
            Self::ValidationInvalidKeywordReferenceRoot => "WF2009",
            Self::ValidationMissingInputDeclaration => "WF2010",
            Self::ValidationMissingSecretsDeclaration => "WF2011",
            Self::ValidationUnknownInputFieldReference => "WF2012",
            Self::ValidationUnknownSecretsFieldReference => "WF2013",
            Self::ValidationSecretReferenceLeak => "WF2014",
            Self::ValidationMissingAgentOutputType => "WF2015",
            Self::ValidationInvalidReferencePath => "WF2016",
            Self::ValidationUnknownSchemaReference => "WF2017",
            Self::ValidationAgentDependencyCycle => "WF2018",
            Self::RuntimeInvalidWorkflow => "WF3000",
            Self::RuntimeExecutionPlanInvariant => "WF3001",
            Self::RuntimeMissingDeclaration => "WF3002",
            Self::RuntimeUnsupportedFeature => "WF3003",
            Self::RuntimeProviderConfiguration => "WF3004",
            Self::RuntimeExpressionEvaluation => "WF3005",
            Self::RuntimeInvalidAgentProperty => "WF3006",
            Self::RuntimeInputTypeMismatch => "WF3007",
            Self::RuntimeOutputTypeMismatch => "WF3008",
            Self::RuntimeInputValueMismatch => "WF3009",
            Self::RuntimeAgentOutputTypeMismatch => "WF3010",
            Self::RuntimeAgentExecutionFailed => "WF3011",
            Self::RuntimeSerializationFailed => "WF3012",
            Self::RuntimeOutputDeserializationFailed => "WF3013",
            Self::RuntimeOther => "WF3999",
        }
    }

    #[must_use]
    pub fn category_description(self) -> &'static str {
        match self {
            Self::ParsePest | Self::ParseMissingNode | Self::ParseUnexpectedRule | Self::ParseInvalidIntegerLiteral => "parser",
            Self::ValidationDuplicateProvider
            | Self::ValidationDuplicateSchema
            | Self::ValidationDuplicateAgent
            | Self::ValidationDuplicateSingleton
            | Self::ValidationInvalidModelExpression
            | Self::ValidationUnknownProviderInModel
            | Self::ValidationUnknownModelForProvider
            | Self::ValidationUnknownAgentReference
            | Self::ValidationInvalidKeywordReferenceRoot
            | Self::ValidationMissingInputDeclaration
            | Self::ValidationMissingSecretsDeclaration
            | Self::ValidationUnknownInputFieldReference
            | Self::ValidationUnknownSecretsFieldReference
            | Self::ValidationSecretReferenceLeak
            | Self::ValidationMissingAgentOutputType
            | Self::ValidationInvalidReferencePath
            | Self::ValidationUnknownSchemaReference
            | Self::ValidationAgentDependencyCycle => "validation",
            Self::RuntimeInvalidWorkflow
            | Self::RuntimeExecutionPlanInvariant
            | Self::RuntimeMissingDeclaration
            | Self::RuntimeUnsupportedFeature
            | Self::RuntimeProviderConfiguration
            | Self::RuntimeExpressionEvaluation
            | Self::RuntimeInvalidAgentProperty
            | Self::RuntimeInputTypeMismatch
            | Self::RuntimeOutputTypeMismatch
            | Self::RuntimeInputValueMismatch
            | Self::RuntimeAgentOutputTypeMismatch
            | Self::RuntimeAgentExecutionFailed
            | Self::RuntimeSerializationFailed
            | Self::RuntimeOutputDeserializationFailed
            | Self::RuntimeOther => "runtime",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub span: SourceSpan,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub primary_span: Option<SourceSpan>,
    pub labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_primary_span(mut self, primary_span: SourceSpan) -> Self {
        self.primary_span = Some(primary_span);
        self
    }

    #[must_use]
    pub fn with_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(DiagnosticLabel {
            span,
            message: message.into(),
        });

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

#[must_use]
pub fn diagnostic_from_parse_error(parse_error: &DslParseError) -> Diagnostic {
    match parse_error {
        DslParseError::Pest { message, span } => Diagnostic::error(DiagnosticCode::ParsePest, message.clone())
            .with_primary_span(*span)
            .with_note("Parser failed while reading workflow source")
            .with_help("Check the highlighted source region for invalid DSL syntax"),
        DslParseError::MissingNode { expected, context, span } => {
            let mut diagnostic = Diagnostic::error(
                DiagnosticCode::ParseMissingNode,
                format!("missing `{expected}` while parsing {context}"),
            )
            .with_help("Ensure the required DSL block or token is present");

            if let Some(span) = span {
                diagnostic = diagnostic.with_primary_span(*span);
            }

            diagnostic
        }
        DslParseError::UnexpectedRule { rule, context, span } => {
            let mut diagnostic = Diagnostic::error(
                DiagnosticCode::ParseUnexpectedRule,
                format!("unexpected parser rule `{rule:?}` while parsing {context}"),
            )
            .with_help("Check workflow syntax near the highlighted location");

            if let Some(span) = span {
                diagnostic = diagnostic.with_primary_span(*span);
            }

            diagnostic
        }
        DslParseError::InvalidIntegerLiteral { literal, context, span } => {
            let mut diagnostic = Diagnostic::error(
                DiagnosticCode::ParseInvalidIntegerLiteral,
                format!("invalid integer literal `{literal}` while parsing {context}"),
            )
            .with_help("Use an integer value that fits into the expected numeric range");

            if let Some(span) = span {
                diagnostic = diagnostic.with_primary_span(*span);
            }

            diagnostic
        }
    }
}

#[must_use]
pub fn diagnostics_from_validation_report(validation_report: &ValidationReport) -> Vec<Diagnostic> {
    validation_report
        .issues_with_spans()
        .map(|(validation_issue, issue_span)| diagnostic_from_validation_issue(validation_issue, issue_span))
        .collect()
}

#[allow(clippy::too_many_lines)]
fn diagnostic_from_validation_issue(validation_issue: &ValidationIssue, issue_span: Option<SourceSpan>) -> Diagnostic {
    let mut diagnostic = match validation_issue {
        ValidationIssue::DuplicateProvider { provider_name } => Diagnostic::error(
            DiagnosticCode::ValidationDuplicateProvider,
            format!("provider `{provider_name}` is declared multiple times"),
        )
        .with_help("Rename the provider or remove the duplicate declaration"),
        ValidationIssue::DuplicateSchema { schema_name } => Diagnostic::error(
            DiagnosticCode::ValidationDuplicateSchema,
            format!("schema `{schema_name}` is declared multiple times"),
        )
        .with_help("Rename the schema or remove the duplicate declaration"),
        ValidationIssue::DuplicateAgent { agent_name } => Diagnostic::error(
            DiagnosticCode::ValidationDuplicateAgent,
            format!("agent `{agent_name}` is declared multiple times"),
        )
        .with_help("Rename the agent or remove the duplicate declaration"),
        ValidationIssue::DuplicateSingletonDeclaration { declaration_kind } => Diagnostic::error(
            DiagnosticCode::ValidationDuplicateSingleton,
            format!(
                "{} declaration appears multiple times",
                render_singleton_declaration_kind(declaration_kind)
            ),
        )
        .with_help("Keep only one declaration block for this singleton section"),
        ValidationIssue::InvalidModelExpression { agent_name } => Diagnostic::error(
            DiagnosticCode::ValidationInvalidModelExpression,
            format!("agent `{agent_name}` has an invalid `model` expression"),
        )
        .with_help("Use `provider_name(\"model\")` for model bindings"),
        ValidationIssue::UnknownProviderInModel { agent_name, provider_name } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownProviderInModel,
            format!("agent `{agent_name}` references unknown provider `{provider_name}` in model binding"),
        )
        .with_help("Declare the provider before agent declarations"),
        ValidationIssue::UnknownModelForProvider {
            agent_name,
            provider_name,
            model_name,
        } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownModelForProvider,
            format!("agent `{agent_name}` references model `{model_name}` not declared by provider `{provider_name}`"),
        )
        .with_help("Add the model name to the provider `models` list"),
        ValidationIssue::UnknownAgentReference { referenced_agent, context } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownAgentReference,
            format!(
                "unknown agent reference `{referenced_agent}` in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Declare the referenced agent before using it"),
        ValidationIssue::InvalidKeywordReferenceRoot { keyword, context } => Diagnostic::error(
            DiagnosticCode::ValidationInvalidKeywordReferenceRoot,
            format!(
                "invalid bare `{}` reference in {}",
                render_reference_keyword(*keyword),
                render_validation_context(context)
            ),
        )
        .with_help("Use a field access such as `input.field` or `agent.name`"),
        ValidationIssue::MissingInputDeclaration { context } => Diagnostic::error(
            DiagnosticCode::ValidationMissingInputDeclaration,
            format!(
                "missing `input` declaration for reference in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Add an `input` block or remove the `input.*` reference"),
        ValidationIssue::MissingSecretsDeclaration { context } => Diagnostic::error(
            DiagnosticCode::ValidationMissingSecretsDeclaration,
            format!(
                "missing `secrets` declaration for reference in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Add a `secrets` block or remove the `secrets.*` reference"),
        ValidationIssue::UnknownInputFieldReference { field_name, context } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownInputFieldReference,
            format!(
                "unknown input field `{field_name}` referenced in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Declare the missing field in the `input` block"),
        ValidationIssue::UnknownSecretsFieldReference { field_name, context } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownSecretsFieldReference,
            format!(
                "unknown secrets field `{field_name}` referenced in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Declare the missing field in the `secrets` block"),
        ValidationIssue::SecretReferenceInLlmContext { reference_path, context } => Diagnostic::error(
            DiagnosticCode::ValidationSecretReferenceLeak,
            format!(
                "secret reference `{reference_path}` is not allowed in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Move secret usage to provider configuration or non-LLM execution paths"),
        ValidationIssue::MissingAgentOutputTypeForFieldReference { agent_name, context } => Diagnostic::error(
            DiagnosticCode::ValidationMissingAgentOutputType,
            format!(
                "agent `{agent_name}` is field-accessed in {}, but no output type is declared",
                render_validation_context(context)
            ),
        )
        .with_help("Add an explicit `output:` type in the referenced agent declaration"),
        ValidationIssue::InvalidReferencePath {
            reference_path,
            invalid_field,
            context,
        } => Diagnostic::error(
            DiagnosticCode::ValidationInvalidReferencePath,
            format!(
                "invalid field `{invalid_field}` in reference `{reference_path}` used in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Verify field names and intermediate object types in the reference path"),
        ValidationIssue::UnknownSchemaReference {
            referenced_schema,
            context,
        } => Diagnostic::error(
            DiagnosticCode::ValidationUnknownSchemaReference,
            format!(
                "unknown schema reference `{referenced_schema}` in {}",
                render_validation_context(context)
            ),
        )
        .with_help("Declare the schema before using it in a type expression"),
        ValidationIssue::AgentDependencyCycle { agent_names } => Diagnostic::error(
            DiagnosticCode::ValidationAgentDependencyCycle,
            format!("agent dependency cycle detected: {}", agent_names.join(", ")),
        )
        .with_help("Break circular dependencies between agent references"),
    };

    if let Some(issue_span) = issue_span {
        diagnostic = diagnostic.with_primary_span(issue_span);
    }

    diagnostic
}

fn render_singleton_declaration_kind(singleton_declaration_kind: &SingletonDeclarationKind) -> &'static str {
    match singleton_declaration_kind {
        SingletonDeclarationKind::Secrets => "`secrets`",
        SingletonDeclarationKind::Input => "`input`",
        SingletonDeclarationKind::Output => "`output`",
    }
}

fn render_validation_context(validation_context: &ValidationContext) -> String {
    match validation_context {
        ValidationContext::Provider(provider_name) => format!("provider `{provider_name}`"),
        ValidationContext::Schema(schema_name) => format!("schema `{schema_name}`"),
        ValidationContext::Agent(agent_name) => format!("agent `{agent_name}`"),
        ValidationContext::Input => "`input` declaration".to_string(),
        ValidationContext::Secrets => "`secrets` declaration".to_string(),
        ValidationContext::Output => "`output` declaration".to_string(),
    }
}

fn render_reference_keyword(reference_keyword: ReferenceKeyword) -> &'static str {
    match reference_keyword {
        ReferenceKeyword::Agent => "agent",
        ReferenceKeyword::Input => "input",
        ReferenceKeyword::Secrets => "secrets",
        ReferenceKeyword::Tool => "tool",
    }
}

#[must_use]
pub fn render_diagnostics(diagnostics: &[Diagnostic], source_code: Option<&str>) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| render_diagnostic(diagnostic, source_code))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[must_use]
pub fn render_diagnostic(diagnostic: &Diagnostic, source_code: Option<&str>) -> String {
    let mut rendered_lines = Vec::new();

    rendered_lines.push(format!(
        "{}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.code.as_str(),
        diagnostic.message
    ));

    if let Some(primary_span) = diagnostic.primary_span {
        rendered_lines.push(format!(" --> {}:{}", primary_span.start.line, primary_span.start.column));

        if let Some(source_code) = source_code {
            rendered_lines.extend(render_code_frame(primary_span, source_code, None));
        }
    }

    if let Some(source_code) = source_code {
        for diagnostic_label in &diagnostic.labels {
            rendered_lines.push(format!(
                " ::: {}:{}",
                diagnostic_label.span.start.line, diagnostic_label.span.start.column
            ));
            rendered_lines.extend(render_code_frame(
                diagnostic_label.span,
                source_code,
                Some(diagnostic_label.message.as_str()),
            ));
        }
    } else {
        for diagnostic_label in &diagnostic.labels {
            rendered_lines.push(format!(
                " ::: {}:{} {}",
                diagnostic_label.span.start.line, diagnostic_label.span.start.column, diagnostic_label.message
            ));
        }
    }

    for note in &diagnostic.notes {
        rendered_lines.push(format!(" note: {note}"));
    }

    if let Some(help) = &diagnostic.help {
        rendered_lines.push(format!(" help: {help}"));
    }

    rendered_lines.join("\n")
}

fn render_code_frame(source_span: SourceSpan, source_code: &str, label_message: Option<&str>) -> Vec<String> {
    let source_lines = source_code.lines().collect::<Vec<_>>();

    if source_lines.is_empty() {
        return Vec::new();
    }

    let first_span_line = source_span.start.line.max(1);
    let last_span_line = source_span.end.line.max(first_span_line);
    let source_line_count = source_lines.len();

    if first_span_line > source_line_count {
        return Vec::new();
    }

    let window_start_line = first_span_line.saturating_sub(1).max(1);
    let window_end_line = (last_span_line + 1).min(source_line_count);
    let line_number_width = window_end_line.to_string().len();
    let mut rendered_lines = Vec::new();

    for line_number in window_start_line..=window_end_line {
        let source_line = source_lines[line_number - 1];
        rendered_lines.push(format!(" {line_number:>line_number_width$} | {source_line}"));

        if line_number < first_span_line || line_number > last_span_line {
            continue;
        }

        let pointer_column_start = pointer_start_column_for_line(source_span, line_number, source_line);
        let pointer_column_end = pointer_end_column_for_line(source_span, line_number, source_line);
        let pointer_width = pointer_column_end.saturating_sub(pointer_column_start).max(1);
        let pointer_padding = " ".repeat(pointer_column_start.saturating_sub(1));
        let pointer_body = "^".repeat(pointer_width);
        let should_attach_label = line_number == first_span_line;

        if should_attach_label {
            if let Some(label_message) = label_message {
                rendered_lines.push(format!(
                    " {:>line_number_width$} | {pointer_padding}{pointer_body} {label_message}",
                    ""
                ));

                continue;
            }
        }

        rendered_lines.push(format!(" {:>line_number_width$} | {pointer_padding}{pointer_body}", ""));
    }

    rendered_lines
}

fn pointer_start_column_for_line(source_span: SourceSpan, line_number: usize, source_line: &str) -> usize {
    let source_line_character_count = source_line.chars().count();
    let max_column = source_line_character_count.saturating_add(1).max(1);

    if line_number == source_span.start.line {
        return source_span.start.column.clamp(1, max_column);
    }

    1
}

fn pointer_end_column_for_line(source_span: SourceSpan, line_number: usize, source_line: &str) -> usize {
    let source_line_character_count = source_line.chars().count();
    let max_column = source_line_character_count.saturating_add(1).max(2);

    if source_span.start.line == source_span.end.line && line_number == source_span.start.line {
        let start_column = source_span.start.column.clamp(1, max_column - 1);
        let end_column = source_span.end.column.clamp(start_column + 1, max_column);

        return end_column;
    }

    if line_number == source_span.start.line {
        return max_column;
    }

    if line_number == source_span.end.line {
        return source_span.end.column.clamp(2, max_column);
    }

    max_column
}

#[cfg(test)]
mod tests {
    use super::{
        diagnostic_from_parse_error, diagnostics_from_validation_report, render_diagnostic, Diagnostic, DiagnosticCode, DiagnosticSeverity,
    };
    use crate::dsl::{parse_workflow, validate_workflow};
    use crate::parse_inline_workflow;
    use std::collections::HashSet;

    #[test]
    fn diagnostic_codes_are_unique_and_follow_workflow_prefix() {
        let diagnostic_codes = [
            DiagnosticCode::ParsePest,
            DiagnosticCode::ParseMissingNode,
            DiagnosticCode::ParseUnexpectedRule,
            DiagnosticCode::ParseInvalidIntegerLiteral,
            DiagnosticCode::ValidationDuplicateProvider,
            DiagnosticCode::ValidationDuplicateSchema,
            DiagnosticCode::ValidationDuplicateAgent,
            DiagnosticCode::ValidationDuplicateSingleton,
            DiagnosticCode::ValidationInvalidModelExpression,
            DiagnosticCode::ValidationUnknownProviderInModel,
            DiagnosticCode::ValidationUnknownModelForProvider,
            DiagnosticCode::ValidationUnknownAgentReference,
            DiagnosticCode::ValidationInvalidKeywordReferenceRoot,
            DiagnosticCode::ValidationMissingInputDeclaration,
            DiagnosticCode::ValidationMissingSecretsDeclaration,
            DiagnosticCode::ValidationUnknownInputFieldReference,
            DiagnosticCode::ValidationUnknownSecretsFieldReference,
            DiagnosticCode::ValidationSecretReferenceLeak,
            DiagnosticCode::ValidationMissingAgentOutputType,
            DiagnosticCode::ValidationInvalidReferencePath,
            DiagnosticCode::ValidationUnknownSchemaReference,
            DiagnosticCode::ValidationAgentDependencyCycle,
            DiagnosticCode::RuntimeInvalidWorkflow,
            DiagnosticCode::RuntimeExecutionPlanInvariant,
            DiagnosticCode::RuntimeMissingDeclaration,
            DiagnosticCode::RuntimeUnsupportedFeature,
            DiagnosticCode::RuntimeProviderConfiguration,
            DiagnosticCode::RuntimeExpressionEvaluation,
            DiagnosticCode::RuntimeInvalidAgentProperty,
            DiagnosticCode::RuntimeInputTypeMismatch,
            DiagnosticCode::RuntimeOutputTypeMismatch,
            DiagnosticCode::RuntimeInputValueMismatch,
            DiagnosticCode::RuntimeAgentOutputTypeMismatch,
            DiagnosticCode::RuntimeAgentExecutionFailed,
            DiagnosticCode::RuntimeSerializationFailed,
            DiagnosticCode::RuntimeOutputDeserializationFailed,
            DiagnosticCode::RuntimeOther,
        ];

        let mut seen_codes = HashSet::new();

        for diagnostic_code in diagnostic_codes {
            assert!(diagnostic_code.as_str().starts_with("WF"));
            assert!(seen_codes.insert(diagnostic_code.as_str()));
            assert!(matches!(
                diagnostic_code.category_description(),
                "parser" | "validation" | "runtime"
            ));
        }
    }

    #[test]
    fn maps_validation_report_to_stable_diagnostics_with_spans() {
        let invalid_workflow = parse_inline_workflow! {
            input {
                title: string
            }

            output {
                broken: input.missing
            }
        };

        let validation_report = validate_workflow(&invalid_workflow);
        let diagnostics = diagnostics_from_validation_report(&validation_report);

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ValidationUnknownInputFieldReference));

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.primary_span.is_some() && diagnostic.severity == DiagnosticSeverity::Error));
    }

    #[test]
    fn maps_parse_error_to_parser_diagnostic_with_span() {
        let parse_error =
            parse_workflow("agent a {\n  prompt: \"hello\"\n}\n@\n").expect_err("workflow should fail to parse for invalid token");

        let diagnostic = diagnostic_from_parse_error(&parse_error);

        assert_eq!(diagnostic.code, DiagnosticCode::ParsePest);
        assert!(diagnostic.primary_span.is_some());
    }

    #[test]
    fn renders_code_frame_with_highlight_markers() {
        let diagnostic = Diagnostic::error(DiagnosticCode::ValidationDuplicateProvider, "duplicate provider").with_primary_span(
            crate::dsl::SourceSpan {
                start: crate::dsl::SourcePosition { line: 2, column: 10 },
                end: crate::dsl::SourcePosition { line: 2, column: 16 },
            },
        );

        let source_code = "provider one {\nprovider one {\n}\n";
        let rendered_output = render_diagnostic(&diagnostic, Some(source_code));

        assert!(rendered_output.contains("error[WF2001]"));
        assert!(rendered_output.contains("2 | provider one {"));
        assert!(rendered_output.contains("^^"));
    }
}
