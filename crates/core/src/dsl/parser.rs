use super::ast::{SourcePosition, SourceSpan, Workflow};
use super::visitor::AstVisitor;
use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use pest::error::{ErrorVariant, LineColLocation};
use pest_derive::Parser;
use thiserror::Error;

#[derive(Parser)]
#[grammar = "dsl/grammar.pest"]
pub struct Parser;

#[derive(Debug, Error)]
pub enum DslParseError {
    #[error("{message}")]
    Pest {
        message: String,
        expected_rules: Vec<Rule>,
        span: SourceSpan,
    },

    #[error("missing {expected} while parsing {context} at {span:?}")]
    MissingNode {
        expected: &'static str,
        context: &'static str,
        span: Option<SourceSpan>,
    },

    #[error("unexpected rule {rule:?} while parsing {context} at {span:?}")]
    UnexpectedRule {
        rule: Rule,
        context: &'static str,
        span: Option<SourceSpan>,
    },

    #[error("invalid integer literal `{literal}` while parsing {context} at {span:?}")]
    InvalidIntegerLiteral {
        literal: String,
        context: &'static str,
        span: Option<SourceSpan>,
    },
}

impl DslParseError {
    #[must_use]
    pub fn from_pest_error(parse_error: pest::error::Error<Rule>) -> Self {
        let expected_rules = match &parse_error.variant {
            ErrorVariant::ParsingError { positives, negatives: _ } => positives.clone(),
            ErrorVariant::CustomError { message: _ } => Vec::new(),
        };

        let span = match parse_error.line_col {
            LineColLocation::Pos((line, column)) => SourceSpan {
                start: SourcePosition { line, column },
                end: SourcePosition { line, column },
            },
            LineColLocation::Span((start_line, start_column), (end_line, end_column)) => SourceSpan {
                start: SourcePosition {
                    line: start_line,
                    column: start_column,
                },
                end: SourcePosition {
                    line: end_line,
                    column: end_column,
                },
            },
        };

        let message = match parse_error.variant {
            ErrorVariant::ParsingError {
                positives: _,
                negatives: _,
            } => "failed to parse DSL".to_string(),
            ErrorVariant::CustomError { message } => message,
        };

        Self::Pest {
            message,
            expected_rules,
            span,
        }
    }

    #[must_use]
    pub fn missing(expected: &'static str, context: &'static str) -> Self {
        Self::MissingNode {
            expected,
            context,
            span: None,
        }
    }

    #[must_use]
    pub fn missing_with_span(expected: &'static str, context: &'static str, span: SourceSpan) -> Self {
        Self::MissingNode {
            expected,
            context,
            span: Some(span),
        }
    }

    #[must_use]
    pub fn unexpected(rule: Rule, context: &'static str) -> Self {
        Self::UnexpectedRule { rule, context, span: None }
    }

    #[must_use]
    pub fn unexpected_with_span(rule: Rule, context: &'static str, span: SourceSpan) -> Self {
        Self::UnexpectedRule {
            rule,
            context,
            span: Some(span),
        }
    }

    pub fn invalid_integer_literal(literal: impl Into<String>, context: &'static str) -> Self {
        Self::InvalidIntegerLiteral {
            literal: literal.into(),
            context,
            span: None,
        }
    }

    pub fn invalid_integer_literal_with_span(literal: impl Into<String>, context: &'static str, span: SourceSpan) -> Self {
        Self::InvalidIntegerLiteral {
            literal: literal.into(),
            context,
            span: Some(span),
        }
    }

    #[must_use]
    pub fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::Pest { span, .. } => Some(*span),
            Self::MissingNode { span, .. } => *span,
            Self::UnexpectedRule { span, .. } => *span,
            Self::InvalidIntegerLiteral { span, .. } => *span,
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Pest {
                message,
                expected_rules,
                span,
            } => {
                let mut diagnostic = Diagnostic::new(DiagnosticCode::from(self), DiagnosticSeverity::Error, message.clone(), Some(*span));

                if !expected_rules.is_empty() {
                    diagnostic = diagnostic.with_note(format!("expected {}", Self::format_expected_rule_list(expected_rules)));
                }

                diagnostic = diagnostic.with_help("Check for a typo, missing `:`, or unmatched `{}` around this location.");

                diagnostic
            }
            Self::MissingNode {
                expected,
                context,
                span: _,
            } => Diagnostic::new(DiagnosticCode::from(self), DiagnosticSeverity::Error, self.to_string(), self.span()).with_help(format!(
                "Add `{expected}` while parsing {context}; this node is required by the DSL grammar."
            )),
            Self::UnexpectedRule { rule, context, span: _ } => {
                Diagnostic::new(DiagnosticCode::from(self), DiagnosticSeverity::Error, self.to_string(), self.span()).with_help(format!(
                    "Remove or reposition `{}` while parsing {context}.",
                    format!("{rule:?}").replace('_', " ")
                ))
            }
            Self::InvalidIntegerLiteral {
                literal,
                context: _,
                span: _,
            } => Diagnostic::new(DiagnosticCode::from(self), DiagnosticSeverity::Error, self.to_string(), self.span()).with_help(format!(
                "Use a non-negative integer that fits within `u64`; `{literal}` is out of range or invalid."
            )),
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        self.diagnostic().render()
    }

    #[must_use]
    pub fn render_with_source(&self, source_text: &str, source_name: &str) -> String {
        self.diagnostic().render_with_source(source_text, source_name)
    }

    fn format_expected_rule_list(expected_rules: &[Rule]) -> String {
        let mut rendered_rule_names = Vec::new();

        for expected_rule in expected_rules {
            let Some(rendered_rule_name) = Self::render_expected_rule_name(*expected_rule) else {
                continue;
            };

            if rendered_rule_names.contains(&rendered_rule_name) {
                continue;
            }

            rendered_rule_names.push(rendered_rule_name);
        }

        match rendered_rule_names.as_slice() {
            [] => "tokens".to_string(),
            [only_rule_name] => only_rule_name.clone(),
            _ => {
                let last_rule_name = rendered_rule_names.pop().expect("last rule name should exist");

                format!("{} or {last_rule_name}", rendered_rule_names.join(", "))
            }
        }
    }

    fn render_expected_rule_name(rule: Rule) -> Option<String> {
        let rule_name = format!("{rule:?}");

        if rule_name == "custom_property" {
            return None;
        }

        let rendered_rule_name = if let Some(property_name) = rule_name.strip_suffix("_property") {
            property_name.replace('_', " ")
        } else {
            rule_name.replace('_', " ")
        };

        Some(format!("`{rendered_rule_name}`"))
    }
}

impl From<&DslParseError> for DiagnosticCode {
    fn from(parse_error: &DslParseError) -> Self {
        match parse_error {
            DslParseError::Pest {
                message: _,
                expected_rules: _,
                span: _,
            } => Self::ParseError,
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

pub fn parse_workflow(source: &str) -> Result<Workflow, DslParseError> {
    let mut parsed_pairs = <Parser as pest::Parser<Rule>>::parse(Rule::workflow, source).map_err(DslParseError::from_pest_error)?;

    let workflow_pair = parsed_pairs
        .next()
        .ok_or_else(|| DslParseError::missing("workflow", "workflow root"))?;

    AstVisitor::new()
        .visit_workflow(workflow_pair)
        .map(|workflow| workflow.with_source_text(source.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::parse_workflow;
    use crate::dsl::macros::parse_inline_workflow;
    use crate::dsl::{
        AgentProperty, CallArgument, Declaration, DslParseError, Expression, ReferenceKeyword, ReferenceRoot, StringTemplatePart,
        TypeExpression,
    };
    use crate::workflow_source;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_all_workflow_samples() {
        for (file_name, workflow_source) in discover_workflow_examples() {
            let workflow =
                parse_workflow(&workflow_source).unwrap_or_else(|parse_error| panic!("{file_name} failed to parse: {parse_error}"));

            assert!(
                !workflow.declarations.is_empty(),
                "{file_name} should produce at least one declaration"
            );
        }
    }

    #[test]
    fn parses_minimum_workflow_structure() {
        let minimum_workflow = parse_inline_workflow! {
            provider ollama {
                driver: "ollama"
                models: ["qwen3.5:32b"]
            }

            agent greeting {
                model: ollama("qwen3.5:32b")
                prompt: "Write a short welcome message."
                output: string
            }

            output {
                greeting: agent.greeting
            }
        };

        assert_eq!(minimum_workflow.declarations.len(), 3);

        match &minimum_workflow.declarations[0] {
            Declaration::Provider(provider_declaration) => {
                assert_eq!(provider_declaration.name, "ollama");
                assert_eq!(provider_declaration.properties.len(), 2);
            }
            _ => panic!("first declaration should be provider"),
        }

        match &minimum_workflow.declarations[1] {
            Declaration::Agent(agent_declaration) => {
                assert_eq!(agent_declaration.name, "greeting");
                assert_eq!(agent_declaration.for_loop, None);
                assert_eq!(agent_declaration.properties.len(), 3);
            }
            _ => panic!("second declaration should be agent"),
        }

        match &minimum_workflow.declarations[2] {
            Declaration::Output(output_declaration) => {
                assert_eq!(output_declaration.fields.len(), 1);
                assert_eq!(output_declaration.fields[0].name, "greeting");
            }
            _ => panic!("third declaration should be output"),
        }
    }

    #[test]
    fn parses_for_loop_agent_structure() {
        let workflow = parse_inline_workflow! {
            agent remediation_plan for finding in agent.findings.items {
                model: ollama("qwen3:8b")
                prompt: "Create a remediation plan for finding: {{ finding }}"
                output: string
            }
        };

        let remediation_plan_agent = workflow
            .find_agent("remediation_plan")
            .expect("missing agent declaration: remediation_plan");

        let loop_definition = remediation_plan_agent
            .for_loop
            .as_ref()
            .expect("remediation_plan should include a for-loop");

        assert_eq!(loop_definition.iterator_name, "finding");

        match &loop_definition.iterable {
            Expression::Reference(reference) => {
                assert_eq!(reference.root, ReferenceRoot::Keyword(ReferenceKeyword::Agent));
                assert_eq!(reference.accesses.len(), 2);
                assert_eq!(reference.accesses[0].field, "findings");
                assert!(!reference.accesses[0].optional);
                assert_eq!(reference.accesses[1].field, "items");
                assert!(!reference.accesses[1].optional);
            }
            _ => panic!("loop iterable should be a reference"),
        }
    }

    #[test]
    fn parses_tools_calls_and_named_arguments() {
        let workflow = parse_inline_workflow! {
            agent assistant_with_tools {
                tools: [
                    tool.web_search,
                    tool.knowledge_base_search(password: secrets.knowledge_base_password),
                    tool.issue_tracker_lookup(project: "engine-ai", status: "open", token: secrets.issue_tracker_token)
                ]
            }
        };

        let tools_agent = workflow
            .find_agent("assistant_with_tools")
            .expect("missing agent declaration: assistant_with_tools");

        let tools_property = tools_agent
            .properties
            .iter()
            .find(|agent_property| matches!(agent_property, AgentProperty::Tools(_)))
            .expect("tools property should exist");

        let AgentProperty::Tools(tools_expression) = tools_property else {
            unreachable!("tools property matcher should guarantee variant");
        };

        let Expression::ArrayLiteral(tools_entries) = tools_expression else {
            panic!("tools property should be an array literal");
        };

        assert_eq!(tools_entries.len(), 3);

        match &tools_entries[1] {
            Expression::FunctionCall(function_call) => {
                assert_eq!(function_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Tool));
                assert_eq!(function_call.callee.accesses[0].field, "knowledge_base_search");
                assert_eq!(function_call.arguments.len(), 1);

                match &function_call.arguments[0] {
                    CallArgument::Named(named_argument) => {
                        assert_eq!(named_argument.name, "password");
                    }
                    _ => panic!("knowledge_base_search argument should be named"),
                }
            }
            _ => panic!("second tools entry should be function call"),
        }
    }

    #[test]
    fn parses_schema_union_types_and_optional_access() {
        let workflow = parse_inline_workflow! {
            schema AllTypes {
                nullable_object: {
                    string_value: string
                    number_value: number
                } | null
            }

            output {
                nullable_object_string: agent.typed_example.nullable_object?.string_value
            }
        };

        let all_types_schema = workflow.find_schema("AllTypes").expect("missing schema declaration: AllTypes");

        let nullable_object_field = all_types_schema
            .fields
            .iter()
            .find(|typed_field| typed_field.name == "nullable_object")
            .expect("nullable_object field should exist");

        match &nullable_object_field.field_type {
            TypeExpression::Union(union_types) => {
                assert_eq!(union_types.len(), 2);
                assert!(union_types
                    .iter()
                    .any(|type_expression| matches!(type_expression, TypeExpression::Object(_))));
                assert!(union_types
                    .iter()
                    .any(|type_expression| matches!(type_expression, TypeExpression::Null)));
            }
            _ => panic!("nullable_object should be parsed as a union type"),
        }

        let output_declaration = workflow.find_output().expect("output declaration should exist");

        let nullable_object_string_field = output_declaration
            .fields
            .iter()
            .find(|object_field| object_field.name == "nullable_object_string")
            .expect("nullable_object_string field should exist");

        match &nullable_object_string_field.value {
            Expression::Reference(reference) => {
                assert!(reference.accesses.iter().any(|reference_access| reference_access.optional));
            }
            _ => panic!("nullable_object_string should be a reference expression"),
        }
    }

    #[test]
    fn parses_output_string_enum_references() {
        let workflow = parse_inline_workflow! {
            input {
                models: {
                    large: string
                    small: string
                }
            }

            agent router {
                output: {
                    model: input.models.large | input.models.small
                }
            }
        };

        let router_agent = workflow.find_agent("router").expect("missing agent declaration: router");
        let output_type = router_agent.output_type().expect("router output type should exist");

        let TypeExpression::Object(output_fields) = output_type else {
            panic!("router output type should be object");
        };

        let model_field = output_fields
            .iter()
            .find(|typed_field| typed_field.name == "model")
            .expect("model field should exist");

        let TypeExpression::Union(union_members) = &model_field.field_type else {
            panic!("model field type should be union");
        };

        assert_eq!(union_members.len(), 2);
        assert!(matches!(
            &union_members[0],
            TypeExpression::StringEnumReference(reference)
                if reference.root == ReferenceRoot::Keyword(ReferenceKeyword::Input)
        ));
        assert!(matches!(
            &union_members[1],
            TypeExpression::StringEnumReference(reference)
                if reference.root == ReferenceRoot::Keyword(ReferenceKeyword::Input)
        ));
    }

    #[test]
    fn parses_string_interpolation_as_structured_template_parts() {
        let workflow = parse_inline_workflow! {
            agent interpolation_test {
                prompt: "A {{ agent.alpha.summary }} B {{ input.topic }} C"
                output: string
            }
        };

        let interpolation_agent = workflow
            .find_agent("interpolation_test")
            .expect("missing agent declaration: interpolation_test");

        let prompt_property = interpolation_agent
            .properties
            .iter()
            .find(|agent_property| matches!(agent_property, AgentProperty::Prompt(_)))
            .expect("prompt property should exist");

        let AgentProperty::Prompt(prompt_expression) = prompt_property else {
            unreachable!("prompt property matcher should guarantee variant");
        };

        let Expression::StringTemplate(prompt_template) = prompt_expression else {
            panic!("prompt should parse as string template");
        };

        assert_eq!(prompt_template.parts.len(), 5);

        assert!(matches!(
            &prompt_template.parts[0],
            StringTemplatePart::Text(text) if text == "A "
        ));

        assert!(matches!(
            &prompt_template.parts[1],
            StringTemplatePart::Interpolation(Expression::Reference(reference))
                if reference.root == ReferenceRoot::Keyword(ReferenceKeyword::Agent) && reference.accesses[0].field == "alpha"
        ));

        assert!(matches!(
            &prompt_template.parts[2],
            StringTemplatePart::Text(text) if text == " B "
        ));

        assert!(matches!(
            &prompt_template.parts[3],
            StringTemplatePart::Interpolation(Expression::Reference(reference))
                if reference.root == ReferenceRoot::Keyword(ReferenceKeyword::Input) && reference.accesses[0].field == "topic"
        ));

        assert!(matches!(
            &prompt_template.parts[4],
            StringTemplatePart::Text(text) if text == " C"
        ));
    }

    #[test]
    fn rejects_single_brace_interpolation_in_string_literals() {
        let workflow_source = workflow_source! {
            agent interpolation_test {
                prompt: "A { agent.alpha.summary }"
                output: string
            }
        };

        let parse_result = parse_workflow(workflow_source);

        assert!(parse_result.is_err());
    }

    #[test]
    fn parse_errors_include_span_for_invalid_token() {
        let parse_result = parse_workflow("agent a {\n    prompt: \"hello\"\n}\n@\n");

        let parse_error = parse_result.expect_err("workflow should fail to parse");
        let parse_error_span = parse_error.span().expect("parse errors should include source span");

        assert_eq!(parse_error_span.start.line, 4);
        assert_eq!(parse_error_span.start.column, 1);
    }

    #[test]
    fn parse_errors_include_span_for_invalid_integer_literal() {
        let parse_result = parse_workflow("schema TooLarge {\n    ids: [string; 184467440737095516160]\n}\n");

        let parse_error = parse_result.expect_err("workflow should fail to parse");

        match parse_error {
            DslParseError::InvalidIntegerLiteral { literal, context: _, span } => {
                assert_eq!(literal, "184467440737095516160");

                let integer_span = span.expect("invalid integer diagnostics should include source span");

                assert_eq!(integer_span.start.line, 2);
                assert_eq!(integer_span.start.column, 19);
            }
            _ => panic!("expected invalid integer literal error"),
        }
    }

    fn discover_workflow_examples() -> Vec<(String, String)> {
        let workflows_directory = workflows_directory();
        let mut workflow_paths = Vec::new();

        collect_workflow_paths(&workflows_directory, &mut workflow_paths);
        workflow_paths.sort();

        workflow_paths
            .into_iter()
            .map(|workflow_path| {
                let relative_workflow_path = workflow_path
                    .strip_prefix(&workflows_directory)
                    .expect("workflow path should be under workflows directory")
                    .to_string_lossy()
                    .replace('\\', "/");

                let workflow_source = fs::read_to_string(&workflow_path)
                    .unwrap_or_else(|read_error| panic!("failed to read {relative_workflow_path}: {read_error}"));

                (relative_workflow_path, workflow_source)
            })
            .collect()
    }

    fn collect_workflow_paths(current_directory: &Path, workflow_paths: &mut Vec<PathBuf>) {
        let directory_entries = fs::read_dir(current_directory)
            .unwrap_or_else(|read_error| panic!("failed to read directory {}: {read_error}", current_directory.display()));

        for directory_entry_result in directory_entries {
            let directory_entry = directory_entry_result
                .unwrap_or_else(|read_error| panic!("failed to read directory entry in {}: {read_error}", current_directory.display()));

            let entry_path = directory_entry.path();

            if entry_path.is_dir() {
                collect_workflow_paths(&entry_path, workflow_paths);

                continue;
            }

            if entry_path.extension().and_then(|extension| extension.to_str()) != Some("ai") {
                continue;
            }

            workflow_paths.push(entry_path);
        }
    }

    fn workflows_directory() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("workflows")
    }
}
