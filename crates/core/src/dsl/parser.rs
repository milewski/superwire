use super::ast::Workflow;
use super::visitor::AstVisitor;
use pest_derive::Parser;
use thiserror::Error;

#[derive(Parser)]
#[grammar = "dsl/grammar.pest"]
pub struct Parser;

#[derive(Debug, Error)]
pub enum DslParseError {
    #[error("failed to parse DSL: {message}")]
    Pest { message: String },

    #[error("missing {expected} while parsing {context}")]
    MissingNode { expected: &'static str, context: &'static str },

    #[error("unexpected rule {rule:?} while parsing {context}")]
    UnexpectedRule { rule: Rule, context: &'static str },

    #[error("invalid integer literal `{literal}` while parsing {context}")]
    InvalidIntegerLiteral { literal: String, context: &'static str },
}

impl DslParseError {
    #[must_use]
    pub fn from_pest_error(parse_error: pest::error::Error<Rule>) -> Self {
        Self::Pest {
            message: parse_error.to_string(),
        }
    }

    #[must_use]
    pub fn missing(expected: &'static str, context: &'static str) -> Self {
        Self::MissingNode { expected, context }
    }

    #[must_use]
    pub fn unexpected(rule: Rule, context: &'static str) -> Self {
        Self::UnexpectedRule { rule, context }
    }

    pub fn invalid_integer_literal(literal: impl Into<String>, context: &'static str) -> Self {
        Self::InvalidIntegerLiteral {
            literal: literal.into(),
            context,
        }
    }
}

pub fn parse_workflow(source: &str) -> Result<Workflow, DslParseError> {
    let mut parsed_pairs = <Parser as pest::Parser<Rule>>::parse(Rule::workflow, source).map_err(DslParseError::from_pest_error)?;

    let workflow_pair = parsed_pairs
        .next()
        .ok_or_else(|| DslParseError::missing("workflow", "workflow root"))?;

    AstVisitor::new().visit_workflow(workflow_pair)
}

#[cfg(test)]
mod tests {
    use super::parse_workflow;
    use crate::dsl::macros::parse_inline_workflow;
    use crate::dsl::{AgentProperty, CallArgument, Declaration, Expression, Reference, StringTemplatePart, TypeExpression};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_all_workflow_samples() {
        for (file_name, workflow_source) in discover_workflow_examples() {
            let workflow =
                parse_workflow(&workflow_source).unwrap_or_else(|parse_error| panic!("{} failed to parse: {parse_error}", file_name));

            assert!(
                !workflow.declarations.is_empty(),
                "{} should produce at least one declaration",
                file_name
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
                assert_eq!(
                    reference,
                    &Reference {
                        root: "agent".to_owned(),
                        accesses: vec![
                            crate::dsl::ReferenceAccess {
                                field: "findings".to_owned(),
                                optional: false,
                            },
                            crate::dsl::ReferenceAccess {
                                field: "items".to_owned(),
                                optional: false,
                            },
                        ],
                    }
                );
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

        let tools_expression = match tools_property {
            AgentProperty::Tools(tools_expression) => tools_expression,
            _ => unreachable!("tools property matcher should guarantee variant"),
        };

        let tools_entries = match tools_expression {
            Expression::ArrayLiteral(tools_entries) => tools_entries,
            _ => panic!("tools property should be an array literal"),
        };

        assert_eq!(tools_entries.len(), 3);

        match &tools_entries[1] {
            Expression::FunctionCall(function_call) => {
                assert_eq!(function_call.callee.root, "tool");
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

        let prompt_expression = match prompt_property {
            AgentProperty::Prompt(prompt_expression) => prompt_expression,
            _ => unreachable!("prompt property matcher should guarantee variant"),
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
                if reference.root == "agent" && reference.accesses[0].field == "alpha"
        ));

        assert!(matches!(
            &prompt_template.parts[2],
            StringTemplatePart::Text(text) if text == " B "
        ));

        assert!(matches!(
            &prompt_template.parts[3],
            StringTemplatePart::Interpolation(Expression::Reference(reference))
                if reference.root == "input" && reference.accesses[0].field == "topic"
        ));

        assert!(matches!(
            &prompt_template.parts[4],
            StringTemplatePart::Text(text) if text == " C"
        ));
    }

    #[test]
    fn rejects_single_brace_interpolation_in_string_literals() {
        let parse_result = parse_workflow(
            r#"
            agent interpolation_test {
                prompt: "A { agent.alpha.summary }"
                output: string
            }
            "#,
        );

        assert!(parse_result.is_err());
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
