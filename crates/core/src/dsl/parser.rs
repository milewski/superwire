use super::ast::{SourcePosition, SourceSpan, Workflow};
use super::visitor::AstVisitor;
use crate::diagnostic::should_render_rich_diagnostics;
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

    #[must_use]
    pub fn render_for_output_target(&self, source_text: &str, source_name: &str) -> String {
        if should_render_rich_diagnostics() {
            return self.render_with_source(source_text, source_name);
        }

        self.render()
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
        AgentExpressionPropertyName, AgentForLoopPattern, AgentProperty, Declaration, DslParseError, Expression, McpCallOperation,
        McpImportKind, McpServerPropertyName, ReferenceKeyword, ReferenceRoot, StringTemplatePart, ToolSource, TypeExpression,
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
            provider ollama from ollama {}

            model ollama_model from ollama {
                id: "qwen3.5:32b"
            }

            agent greeting {
                model: model.ollama_model
                instruction: "Write a short welcome message."
                output {
                    value: string
                }
            }

            output {
                greeting: agent.greeting.value
            }
        };

        assert_eq!(minimum_workflow.declarations.len(), 4);

        match &minimum_workflow.declarations[0] {
            Declaration::Provider(provider_declaration) => {
                assert_eq!(provider_declaration.name, "ollama");
                assert_eq!(provider_declaration.driver_name, "ollama");
                assert_eq!(provider_declaration.properties.len(), 0);
            }
            _ => panic!("first declaration should be provider"),
        }

        match &minimum_workflow.declarations[1] {
            Declaration::Model(model_declaration) => {
                assert_eq!(model_declaration.name, "ollama_model");
                assert_eq!(model_declaration.provider_name, "ollama");
            }
            _ => panic!("second declaration should be model"),
        }

        match &minimum_workflow.declarations[2] {
            Declaration::Agent(agent_declaration) => {
                assert_eq!(agent_declaration.name, "greeting");
                assert_eq!(agent_declaration.for_loop, None);
                assert_eq!(agent_declaration.properties.len(), 3);
            }
            _ => panic!("third declaration should be agent"),
        }

        match &minimum_workflow.declarations[3] {
            Declaration::Output(output_declaration) => {
                assert_eq!(output_declaration.fields.len(), 1);
                assert_eq!(output_declaration.fields[0].name, "greeting");
            }
            _ => panic!("fourth declaration should be output"),
        }
    }

    #[test]
    fn parses_for_loop_agent_structure() {
        let workflow = parse_inline_workflow! {
            agent remediation_plan for finding in agent.findings.items {
                model: model.ollama_model
                instruction: "Create a remediation plan for finding: {{ finding }}"
                output {
                    value: string
                }
            }
        };

        let remediation_plan_agent = workflow
            .find_agent("remediation_plan")
            .expect("missing agent declaration: remediation_plan");

        let loop_definition = remediation_plan_agent
            .for_loop
            .as_ref()
            .expect("remediation_plan should include a for-loop");

        assert_eq!(loop_definition.pattern, AgentForLoopPattern::Identifier("finding".to_string()));

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
    fn parses_object_destructuring_for_loop_agent_structure() {
        let workflow = parse_inline_workflow! {
            agent participant_analyzer for { id, name } in agent.alpha.participants {
                instruction: "Analyze participant {{ id }} and {{ name }}"
                output {
                    value: string
                }
            }
        };

        let participant_analyzer_agent = workflow
            .find_agent("participant_analyzer")
            .expect("missing agent declaration: participant_analyzer");

        let loop_definition = participant_analyzer_agent
            .for_loop
            .as_ref()
            .expect("participant_analyzer should include a for-loop");

        assert_eq!(
            loop_definition.pattern,
            AgentForLoopPattern::ObjectDestructuring(vec!["id".to_string(), "name".to_string()])
        );

        match &loop_definition.iterable {
            Expression::Reference(reference) => {
                assert_eq!(reference.root, ReferenceRoot::Keyword(ReferenceKeyword::Agent));
                assert_eq!(reference.accesses.len(), 2);
                assert_eq!(reference.accesses[0].field, "alpha");
                assert_eq!(reference.accesses[1].field, "participants");
            }
            _ => panic!("loop iterable should be a reference"),
        }
    }

    #[test]
    fn parses_reference_paths_with_keyword_roots_and_optional_accesses() {
        struct ExpectedReferenceAccess {
            field: &'static str,
            optional: bool,
        }

        struct ReferenceParseCase {
            output_field_name: &'static str,
            root_keyword: ReferenceKeyword,
            accesses: &'static [ExpectedReferenceAccess],
            rendered_path: &'static str,
        }

        let workflow = parse_inline_workflow! {
            input {
                topic: string
            }

            agent reviewer {
                instruction: input.topic

                output {
                    value: maybe string
                }
            }

            output {
                topic: input.topic
                required_agent_value: agent.reviewer.value
                optional_agent_value: agent.reviewer?.value
                secret_value: secrets.api_key
            }
        };
        let output_declaration = workflow.find_output().expect("workflow should include output declaration");
        let reference_parse_cases = [
            ReferenceParseCase {
                output_field_name: "topic",
                root_keyword: ReferenceKeyword::Input,
                accesses: &[ExpectedReferenceAccess {
                    field: "topic",
                    optional: false,
                }],
                rendered_path: "input.topic",
            },
            ReferenceParseCase {
                output_field_name: "required_agent_value",
                root_keyword: ReferenceKeyword::Agent,
                accesses: &[
                    ExpectedReferenceAccess {
                        field: "reviewer",
                        optional: false,
                    },
                    ExpectedReferenceAccess {
                        field: "value",
                        optional: false,
                    },
                ],
                rendered_path: "agent.reviewer.value",
            },
            ReferenceParseCase {
                output_field_name: "optional_agent_value",
                root_keyword: ReferenceKeyword::Agent,
                accesses: &[
                    ExpectedReferenceAccess {
                        field: "reviewer",
                        optional: false,
                    },
                    ExpectedReferenceAccess {
                        field: "value",
                        optional: true,
                    },
                ],
                rendered_path: "agent.reviewer?.value",
            },
            ReferenceParseCase {
                output_field_name: "secret_value",
                root_keyword: ReferenceKeyword::Secrets,
                accesses: &[ExpectedReferenceAccess {
                    field: "api_key",
                    optional: false,
                }],
                rendered_path: "secrets.api_key",
            },
        ];

        for reference_parse_case in reference_parse_cases {
            let output_field = output_declaration
                .fields
                .iter()
                .find(|output_field| output_field.name == reference_parse_case.output_field_name)
                .expect("output field should exist");
            let Expression::Reference(reference) = &output_field.value else {
                panic!("output field should be a reference");
            };

            assert_eq!(reference.root, ReferenceRoot::Keyword(reference_parse_case.root_keyword));
            assert_eq!(reference.accesses.len(), reference_parse_case.accesses.len());

            for (reference_access, expected_access) in reference.accesses.iter().zip(reference_parse_case.accesses) {
                assert_eq!(reference_access.field, expected_access.field);
                assert_eq!(reference_access.optional, expected_access.optional);
            }

            assert_eq!(reference.render_path(), reference_parse_case.rendered_path);
        }
    }

    #[test]
    fn parses_tools_entries_and_binding_overrides() {
        let workflow = parse_inline_workflow! {
            agent assistant_with_tools {
                uses: [
                    tool.web_search,
                    tool.knowledge_base_search {
                        bindings {
                            password: secrets.knowledge_base_password
                        }
                    },
                    tool.issue_tracker_lookup {
                        bindings {
                            project: "engine-ai"
                            status: "open"
                            token: secrets.issue_tracker_token
                        }
                    }
                ]
            }
        };

        let tools_agent = workflow
            .find_agent("assistant_with_tools")
            .expect("missing agent declaration: assistant_with_tools");

        let uses_property = tools_agent
            .properties
            .iter()
            .find(|agent_property| matches!(agent_property, AgentProperty::Uses(_)))
            .expect("uses property should exist");

        let AgentProperty::Uses(uses_expression) = uses_property else {
            unreachable!("uses property matcher should guarantee variant");
        };

        let Expression::ArrayLiteral(tools_entries) = uses_expression else {
            panic!("uses property should be an array literal");
        };

        assert_eq!(tools_entries.len(), 3);

        match &tools_entries[1] {
            Expression::ToolCall(tool_call) => {
                assert_eq!(tool_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Tool));
                assert_eq!(tool_call.callee.accesses[0].field, "knowledge_base_search");
                assert_eq!(tool_call.binding_fields.len(), 1);
                assert_eq!(tool_call.binding_fields[0].name, "password");
            }
            _ => panic!("second tools entry should be tool binding"),
        }
    }

    #[test]
    fn rejects_call_style_tool_binding_overrides_inside_uses_property() {
        let workflow_source = workflow_source! {
            agent assistant_with_tools {
                uses: [
                    tool.knowledge_base_search(password: secrets.knowledge_base_password)
                ]
            }
        };

        assert!(parse_workflow(workflow_source).is_err());
    }

    #[test]
    fn parses_mcp_headers_as_block_property() {
        let workflow = parse_inline_workflow! {
            mcp local {
                endpoint: secrets.mcp_summarizer_endpoint
                headers {
                    Accept: "application/json"
                    Authorization: "Bearer {{ secrets.mcp_token }}"
                }
            }
        };

        let mcp_server = workflow.find_mcp_server("local").expect("MCP server should parse");
        let headers_property = mcp_server
            .properties
            .iter()
            .find(|property| McpServerPropertyName::from_identifier(&property.name) == Some(McpServerPropertyName::Headers))
            .expect("headers property should parse");

        let Expression::ObjectLiteral(header_fields) = &headers_property.value else {
            panic!("headers should parse as object literal");
        };

        assert_eq!(header_fields.len(), 2);
    }

    #[test]
    fn rejects_mcp_headers_colon_property() {
        let workflow_source = workflow_source! {
            mcp local {
                endpoint: secrets.mcp_summarizer_endpoint
                headers: {
                    Accept: "application/json"
                }
            }
        };

        assert!(parse_workflow(workflow_source).is_err());
    }

    #[test]
    fn parses_tool_declarations_with_input_bindings_and_output_fields() {
        let workflow = parse_inline_workflow! {
            tool web_search {
                query: string
            }

            tool issue_tracker_lookup {
                description: "retrieve details about an issue"

                input {
                    issue_id: number
                }

                bindings {
                    project: input.project,
                    endpoint: "https://example.test",
                    status: "open",
                    token: input.token,
                }

                output {
                    title: string
                }
            }
        };

        let web_search_tool = workflow.find_tool("web_search").expect("missing web_search tool declaration");

        assert_eq!(web_search_tool.input_fields.len(), 1);
        assert_eq!(web_search_tool.input_fields[0].name, "query");

        let issue_tracker_tool = workflow
            .find_tool("issue_tracker_lookup")
            .expect("missing issue_tracker_lookup tool declaration");

        assert_eq!(issue_tracker_tool.description.as_deref(), Some("retrieve details about an issue"));
        assert_eq!(issue_tracker_tool.input_fields.len(), 1);
        assert!(issue_tracker_tool.binding_fields.is_empty());
        assert_eq!(issue_tracker_tool.fixed_binding_fields.len(), 4);
        assert_eq!(issue_tracker_tool.fixed_binding_fields[0].name, "project");
        assert_eq!(issue_tracker_tool.output_fields.len(), 1);
    }

    #[test]
    fn parses_mcp_first_class_imports_with_aliases_and_bindings() {
        let workflow = parse_inline_workflow! {
            input {
                workspace_id: string
            }

            resource project_readme from mcp.local.resource.project_readme {
                bindings {
                    workspace_id: input.workspace_id
                }
            }

            prompt from mcp.local.prompt.system_prompt

            tool from mcp.local.tool.create_sorting_task_for_task_group_tool {
                bindings {
                    workspace_id: input.workspace_id
                }
            }

            tool create_sorting_task_for_task_group from mcp.local.tool.create_sorting_task_for_task_group_tool {
                bindings {
                    workspace_id: input.workspace_id
                }
            }
        };

        let resource_import = workflow.resource_imports().next().expect("resource import should parse");
        assert_eq!(resource_import.name, "project_readme");
        assert_eq!(resource_import.source.server_name, "local");
        assert_eq!(resource_import.source.kind, McpImportKind::Resource);
        assert_eq!(resource_import.source.item_name, "project_readme");
        assert_eq!(resource_import.parameters.len(), 1);

        let prompt_import = workflow.prompt_imports().next().expect("prompt import should parse");
        assert_eq!(prompt_import.name, "system_prompt");
        assert_eq!(prompt_import.source.kind, McpImportKind::Prompt);
        assert_eq!(prompt_import.source.item_name, "system_prompt");

        let inferred_tool = workflow
            .find_tool("create_sorting_task_for_task_group_tool")
            .expect("inferred tool import should parse");
        assert!(inferred_tool.imported);
        assert_eq!(inferred_tool.fixed_binding_fields.len(), 1);

        let aliased_tool = workflow
            .find_tool("create_sorting_task_for_task_group")
            .expect("aliased tool import should parse");
        assert!(aliased_tool.imported);
        assert!(matches!(
            &aliased_tool.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local")
                    && mcp_tool_source.tool_name == "create_sorting_task_for_task_group_tool"
        ));
    }

    #[test]
    fn parses_mcp_tool_import_with_output_fields() {
        let workflow = parse_inline_workflow! {
            tool fetch_numbers from mcp.local.tool.fetch_numbers {
                output {
                    values: [number]
                }
            }
        };

        let tool_declaration = workflow
            .find_tool("fetch_numbers")
            .expect("tool import with output fields should parse");

        assert!(tool_declaration.imported);
        assert_eq!(tool_declaration.output_fields.len(), 1);
        assert_eq!(tool_declaration.output_fields[0].name, "values");
    }

    #[test]
    fn parses_mcp_tool_batch_imports_with_shared_bindings_and_aliases() {
        let workflow = parse_inline_workflow! {
            from mcp.local.tool {
                bindings {
                    project_id: 1
                    task_id: 2
                }

                max_calls: 3

                tool create_sorting_task
                tool update_task_status {
                    bindings {
                        status: "done"
                    }
                }
                tool assign_task
            }
        };

        assert_eq!(workflow.declarations.len(), 1);
        assert_eq!(workflow.tool_declarations().count(), 3);

        let Declaration::McpToolBatch(tool_batch_import_declaration) = &workflow.declarations[0] else {
            panic!("declaration should be an MCP tool batch import");
        };

        assert_eq!(tool_batch_import_declaration.server_name, "local");
        assert_eq!(tool_batch_import_declaration.fixed_binding_fields.len(), 2);
        assert_eq!(tool_batch_import_declaration.max_calls, Some(3));

        let create_tool = workflow
            .find_tool("create_sorting_task")
            .expect("aliased batch tool should be findable as a tool declaration");
        assert_eq!(create_tool.fixed_binding_fields.len(), 2);
        assert_eq!(create_tool.max_calls, Some(3));
        assert!(matches!(
            &create_tool.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local")
                    && mcp_tool_source.tool_name == "create_sorting_task"
        ));

        let update_tool = workflow
            .find_tool("update_task_status")
            .expect("aliased batch tool with extra bindings should be findable as a tool declaration");
        assert_eq!(update_tool.fixed_binding_fields.len(), 3);
        assert_eq!(update_tool.fixed_binding_fields[0].name, "project_id");
        assert_eq!(update_tool.fixed_binding_fields[1].name, "task_id");
        assert_eq!(update_tool.fixed_binding_fields[2].name, "status");

        let assigned_tool = workflow
            .find_tool("assign_task")
            .expect("non-aliased batch tool should infer a local name");
        assert!(matches!(
            &assigned_tool.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local") && mcp_tool_source.tool_name == "assign_task"
        ));
    }

    #[test]
    fn parses_mcp_resource_and_prompt_batch_imports_with_shared_parameters() {
        let workflow = parse_inline_workflow! {
            from mcp.local.resource {
                bindings {
                    workspace_id: input.workspace_id
                }

                resource task_type_resource
                resource project_readme {
                    bindings {
                        section: "setup"
                    }
                }
            }

            from mcp.local.prompt {
                bindings {
                    workspace_id: input.workspace_id
                }

                prompt task_summary_prompt
                prompt system_prompt {
                    bindings {
                        task_type: "investigation"
                    }
                }
            }
        };

        assert_eq!(workflow.resource_imports().count(), 2);
        assert_eq!(workflow.prompt_imports().count(), 2);

        let task_type_resource = workflow
            .find_resource_import("task_type_resource")
            .expect("resource from batch import should parse");
        assert_eq!(task_type_resource.source.server_name, "local");
        assert_eq!(task_type_resource.parameters.len(), 1);

        let project_readme = workflow
            .find_resource_import("project_readme")
            .expect("resource with item bindings should parse");
        assert_eq!(project_readme.parameters.len(), 2);

        let task_summary_prompt = workflow
            .find_prompt_import("task_summary_prompt")
            .expect("prompt from batch import should parse");
        assert_eq!(task_summary_prompt.source.server_name, "local");
        assert_eq!(task_summary_prompt.parameters.len(), 1);

        let system_prompt = workflow
            .find_prompt_import("system_prompt")
            .expect("prompt with item bindings should parse");
        assert_eq!(system_prompt.parameters.len(), 2);
    }

    #[test]
    fn mcp_batch_item_bindings_override_shared_bindings() {
        let workflow = parse_inline_workflow! {
            from mcp.local {
                bindings {
                    project_id: input.project_id
                }

                prompt dynamic_summary_prompt {
                    bindings {
                        project_id: 123
                    }
                }
            }
        };

        let prompt_import = workflow
            .find_prompt_import("dynamic_summary_prompt")
            .expect("prompt import from mixed MCP batch should parse");

        assert_eq!(prompt_import.parameters.len(), 1);
        assert_eq!(prompt_import.parameters[0].name, "project_id");
        assert!(matches!(
            &prompt_import.parameters[0].value,
            Expression::NumberLiteral(number_literal) if number_literal == "123"
        ));
    }

    #[test]
    fn parses_mixed_mcp_batch_imports_with_shared_bindings() {
        let workflow = parse_inline_workflow! {
            from mcp.local {
                bindings {
                    project_id: input.project_id
                }

                resource all_tasks
                prompt create_task_instructions
                tool create_task
            }
        };

        assert_eq!(workflow.tool_declarations().count(), 1);
        assert_eq!(workflow.resource_imports().count(), 1);
        assert_eq!(workflow.prompt_imports().count(), 1);

        let tool_declaration = workflow.find_tool("create_task").expect("mixed batch tool should parse");
        assert_eq!(tool_declaration.fixed_binding_fields.len(), 1);

        let resource_import = workflow
            .find_resource_import("all_tasks")
            .expect("mixed batch resource should parse");
        assert_eq!(resource_import.parameters.len(), 1);

        let prompt_import = workflow
            .find_prompt_import("create_task_instructions")
            .expect("mixed batch prompt should parse");
        assert_eq!(prompt_import.parameters.len(), 1);
    }

    #[test]
    fn parses_agent_instruction_and_mixed_uses() {
        let workflow = parse_inline_workflow! {
            resource all_tasks from mcp.local.resource.all_tasks
            prompt create_task_instructions from mcp.local.prompt.create_task_instructions
            tool create_task from mcp.local.tool.create_task

            agent task_manager {
                model: model.openai_model
                instruction: "Create a task"
                uses: [tool.create_task, prompt.create_task_instructions, resource.all_tasks]
                output {
                    value: string
                }
            }
        };

        let agent_declaration = workflow.find_agent("task_manager").expect("agent should parse");

        assert!(agent_declaration
            .expression_property(AgentExpressionPropertyName::Instruction)
            .is_some());
        assert!(agent_declaration.expression_property(AgentExpressionPropertyName::Uses).is_some());
    }

    #[test]
    fn parses_resource_read_and_prompt_render_expressions() {
        let workflow = parse_inline_workflow! {
            resource project_readme from mcp.local.resource.project_readme
            prompt system_prompt from mcp.local.prompt.system_prompt

            dynamic {
                readme: read resource.project_readme {
                    bindings {
                        section: "setup"
                    }
                }
                instructions: render prompt.system_prompt {
                    bindings {
                        topic: dynamic.readme
                    }
                }
            }
        };

        let dynamic_block = workflow.dynamic_blocks().next().expect("dynamic block should parse");

        let Expression::McpCall(resource_call) = &dynamic_block.fields[0].value else {
            panic!("readme should be an MCP resource call");
        };

        assert_eq!(resource_call.operation, McpCallOperation::Read);
        assert_eq!(resource_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Resource));
        assert_eq!(resource_call.callee.accesses[0].field, "project_readme");
        assert_eq!(resource_call.parameter_fields[0].name, "section");

        let Expression::McpCall(prompt_call) = &dynamic_block.fields[1].value else {
            panic!("instructions should be an MCP prompt call");
        };

        assert_eq!(prompt_call.operation, McpCallOperation::Render);
        assert_eq!(prompt_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Prompt));
        assert_eq!(prompt_call.callee.accesses[0].field, "system_prompt");
        assert_eq!(prompt_call.parameter_fields[0].name, "topic");
    }

    #[test]
    fn parses_fixed_tool_bindings_from_references_and_literals() {
        let workflow = parse_inline_workflow! {
            input {
                project_id: number
                task_id: number
            }

            tool list_all_participants_who_has_answered_given_task {
                bindings {
                    project_id: input.project_id
                    retry_count: 123
                    task_id: agent.example.task_id
                }

                output {
                    task_title: string
                }
            }
        };

        let tool_declaration = workflow
            .find_tool("list_all_participants_who_has_answered_given_task")
            .expect("missing tool declaration");

        assert!(tool_declaration.binding_fields.is_empty());
        assert_eq!(tool_declaration.fixed_binding_fields.len(), 3);

        let project_binding = &tool_declaration.fixed_binding_fields[0];
        assert_eq!(project_binding.name, "project_id");

        let Expression::Reference(project_reference) = &project_binding.value else {
            panic!("project_id binding should be a reference");
        };

        assert_eq!(project_reference.root, ReferenceRoot::Keyword(ReferenceKeyword::Input));
        assert_eq!(project_reference.accesses[0].field, "project_id");

        let retry_binding = &tool_declaration.fixed_binding_fields[1];
        assert_eq!(retry_binding.name, "retry_count");

        let Expression::NumberLiteral(retry_count) = &retry_binding.value else {
            panic!("retry_count binding should be a number literal");
        };

        assert_eq!(retry_count, "123");

        let task_binding = &tool_declaration.fixed_binding_fields[2];
        assert_eq!(task_binding.name, "task_id");

        let Expression::Reference(task_reference) = &task_binding.value else {
            panic!("task_id binding should be a reference");
        };

        assert_eq!(task_reference.root, ReferenceRoot::Keyword(ReferenceKeyword::Agent));
        assert_eq!(task_reference.accesses[0].field, "example");
        assert_eq!(task_reference.accesses[1].field, "task_id");
    }

    #[test]
    fn parses_single_item_array_fixed_tool_binding() {
        let workflow = parse_inline_workflow! {
            tool fetch_answers {
                bindings {
                    task_types: ["open_written"]
                }
            }
        };

        let tool_declaration = workflow.find_tool("fetch_answers").expect("missing tool declaration");
        let task_types_binding = tool_declaration
            .fixed_binding_fields
            .iter()
            .find(|fixed_binding_field| fixed_binding_field.name == "task_types")
            .expect("task types binding should exist");

        assert!(matches!(&task_types_binding.value, Expression::ArrayLiteral(array_items) if array_items.len() == 1));
    }

    #[test]
    fn parses_dynamic_blocks_and_deterministic_tool_calls() {
        let workflow = parse_inline_workflow! {
            tool fetch_issue {
                description: "Fetch issue"

                bindings {
                    repository: input.repository
                }

                input {
                    sha: string
                }

                output {
                    title: string
                }
            }

            dynamic {
                issue: call tool.fetch_issue {
                    input {
                        sha: input.sha
                    }

                    bindings {
                        repository: input.repository
                    }
                }
            }

            agent summarize {
                dynamic {
                    local_issue: call tool.fetch_issue {
                        input {
                            sha: dynamic.issue.title
                        }
                    }
                }

                instruction: "{{ dynamic.local_issue.title }}"
                output {
                    value: string
                }
            }
        };

        let Declaration::Dynamic(dynamic_block) = &workflow.declarations[1] else {
            panic!("second declaration should be dynamic block");
        };

        assert_eq!(dynamic_block.fields.len(), 1);
        assert_eq!(dynamic_block.fields[0].name, "issue");

        let Expression::ToolCall(tool_call) = &dynamic_block.fields[0].value else {
            panic!("dynamic value should be a tool call");
        };

        assert_eq!(tool_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Tool));
        assert_eq!(tool_call.callee.accesses[0].field, "fetch_issue");
        assert_eq!(tool_call.input_fields.len(), 1);
        assert_eq!(tool_call.binding_fields.len(), 1);
    }

    #[test]
    fn parses_blockless_deterministic_tool_calls() {
        let workflow = parse_inline_workflow! {
            tool list_participants {
                output {
                    count: number
                }
            }

            dynamic {
                data: call tool.list_participants
            }
        };

        let Declaration::Dynamic(dynamic_block) = &workflow.declarations[1] else {
            panic!("second declaration should be dynamic block");
        };

        let Expression::ToolCall(tool_call) = &dynamic_block.fields[0].value else {
            panic!("dynamic value should be a tool call");
        };

        assert_eq!(tool_call.callee.root, ReferenceRoot::Keyword(ReferenceKeyword::Tool));
        assert_eq!(tool_call.callee.accesses[0].field, "list_participants");
        assert!(tool_call.input_fields.is_empty());
        assert!(tool_call.binding_fields.is_empty());
    }

    #[test]
    fn parses_schema_maybe_types_and_optional_access() {
        let workflow = parse_inline_workflow! {
            schema all_types {
                nullable_object: maybe {
                    string_value: string
                    number_value: number
                }
            }

            output {
                nullable_object_string: agent.typed_example.nullable_object?.string_value
            }
        };

        let all_types_schema = workflow.find_schema("all_types").expect("missing schema declaration: all_types");

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
    fn parses_enum_and_variant_type_expressions() {
        let workflow = parse_inline_workflow! {
            schema event_payload {
                status: enum {
                    draft
                    ready
                    published
                }

                payload: variant type {
                    user_created {
                        user_id: string
                    }

                    "user.deleted" {
                        user_id: string
                        reason: maybe string
                    }
                }
            }
        };

        let schema_declaration = workflow
            .find_schema("event_payload")
            .expect("missing schema declaration: event_payload");
        let status_field = schema_declaration
            .fields
            .iter()
            .find(|typed_field| typed_field.name == "status")
            .expect("status field should exist");

        let TypeExpression::Union(enum_members) = &status_field.field_type else {
            panic!("status should parse as enum union");
        };

        assert_eq!(enum_members.len(), 3);
        assert!(matches!(&enum_members[0], TypeExpression::StringEnum(enum_value) if enum_value == "draft"));

        let payload_field = schema_declaration
            .fields
            .iter()
            .find(|typed_field| typed_field.name == "payload")
            .expect("payload field should exist");

        let TypeExpression::Variant { discriminator, cases } = &payload_field.field_type else {
            panic!("payload should parse as variant");
        };

        assert_eq!(discriminator, "type");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].name, "user_created");
        assert_eq!(cases[1].name, "user.deleted");
    }

    #[test]
    fn parses_object_level_schema_variant() {
        let workflow = parse_inline_workflow! {
            schema api_event {
                variant type {
                    user_created {
                        user_id: string
                    }

                    user_deleted {
                        user_id: string
                    }
                }
            }
        };

        let schema_declaration = workflow.find_schema("api_event").expect("missing schema declaration: api_event");

        assert!(schema_declaration.fields.is_empty());
        assert!(matches!(schema_declaration.root_variant, Some(TypeExpression::Variant { .. })));
    }

    #[test]
    fn parses_fallback_projection_and_match_expressions() {
        let workflow = parse_inline_workflow! {
            output {
                projected: agent.events.payload # user_created.user_id ?? "unknown"
                matched: match agent.events.payload {
                    user_created.user_id
                    user_deleted.user_id
                    _ "unknown"
                }
            }
        };

        let output_declaration = workflow.find_output().expect("output declaration should exist");
        let projected_field = output_declaration
            .fields
            .iter()
            .find(|field| field.name == "projected")
            .expect("projected output field should exist");
        let matched_field = output_declaration
            .fields
            .iter()
            .find(|field| field.name == "matched")
            .expect("matched output field should exist");

        assert!(matches!(projected_field.value, Expression::NullFallback(_)));
        assert!(matches!(matched_field.value, Expression::Match(_)));
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
                output {
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
    fn parses_schema_field_string_enum_references() {
        let workflow = parse_inline_workflow! {
            schema main {
                language_enum: enum { en_US, zh_CN, fr }
            }

            tool example {
                input {
                    language: schema.main.language_enum
                }
            }
        };

        let tool_declaration = workflow.find_tool("example").expect("missing tool declaration: example");
        let language_field = tool_declaration
            .input_fields
            .iter()
            .find(|typed_field| typed_field.name == "language")
            .expect("language input field should exist");

        assert!(matches!(
            &language_field.field_type,
            TypeExpression::StringEnumReference(reference)
                if reference.root.as_identifier() == Some("schema")
                    && reference.accesses[0].field == "main"
                    && reference.accesses[1].field == "language_enum"
        ));
    }

    #[test]
    fn parses_string_interpolation_as_structured_template_parts() {
        let workflow = parse_inline_workflow! {
            agent interpolation_test {
                instruction: "A {{ agent.alpha.summary }} B {{ input.topic }} C"
                output {
                    value: string
                }
            }
        };

        let interpolation_agent = workflow
            .find_agent("interpolation_test")
            .expect("missing agent declaration: interpolation_test");

        let instruction_property = interpolation_agent
            .properties
            .iter()
            .find(|agent_property| matches!(agent_property, AgentProperty::Instruction(_)))
            .expect("instruction property should exist");

        let AgentProperty::Instruction(instruction_expression) = instruction_property else {
            unreachable!("instruction property matcher should guarantee variant");
        };

        let Expression::StringTemplate(prompt_template) = instruction_expression else {
            panic!("instruction should parse as string template");
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
                instruction: "A { agent.alpha.summary }"
                output {
                    value: string
                }
            }
        };

        let parse_result = parse_workflow(workflow_source);

        assert!(parse_result.is_err());
    }

    #[test]
    fn parse_errors_include_span_for_invalid_token() {
        let parse_result = parse_workflow("agent a {\n    instruction: \"hello\"\n}\n@\n");

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

    #[test]
    fn rejects_postfix_field_description() {
        let workflow_source = workflow_source! {
            input {
                greeting: string "example"
            }
        };

        assert!(parse_workflow(workflow_source).is_err());
    }

    #[test]
    fn rejects_type_syntax_inside_bindings_blocks() {
        let workflow_source = workflow_source! {
            from mcp.local {
                tool query_docs_filesystem_superwire {
                    bindings {
                        /// Shell command to run.
                        command: string
                    }
                }
            }
        };

        assert!(parse_workflow(workflow_source).is_err());
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
