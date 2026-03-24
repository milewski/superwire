use super::ast::{
    AgentProperty, CallArgument, Declaration, Expression, FunctionCall, ObjectField, Reference, StringTemplatePart, TypeExpression,
    Workflow,
};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationReport {
    issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }

    #[must_use]
    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    #[must_use]
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    fn push_issue(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SingletonDeclarationKind {
    Secrets,
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ValidationContext {
    Provider(String),
    Schema(String),
    Agent(String),
    Input,
    Secrets,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    DuplicateProvider {
        provider_name: String,
    },
    DuplicateSchema {
        schema_name: String,
    },
    DuplicateAgent {
        agent_name: String,
    },
    DuplicateSingletonDeclaration {
        declaration_kind: SingletonDeclarationKind,
    },
    InvalidModelExpression {
        agent_name: String,
    },
    UnknownProviderInModel {
        agent_name: String,
        provider_name: String,
    },
    UnknownModelForProvider {
        agent_name: String,
        provider_name: String,
        model_name: String,
    },
    UnknownAgentReference {
        referenced_agent: String,
        context: ValidationContext,
    },
    UnknownSchemaReference {
        referenced_schema: String,
        context: ValidationContext,
    },
    AgentDependencyCycle {
        agent_names: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct ProviderInfo {
    declared_models: Option<HashSet<String>>,
}

#[derive(Debug, Clone, Default)]
struct ValidationIndex {
    provider_infos: HashMap<String, ProviderInfo>,
    agent_names: HashSet<String>,
    schema_names: HashSet<String>,
}

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut validation_report = ValidationReport::default();
    let validation_index = build_validation_index(workflow, &mut validation_report);

    validate_schema_references(workflow, &validation_index, &mut validation_report);
    validate_agent_model_bindings(workflow, &validation_index, &mut validation_report);
    validate_agent_references(workflow, &validation_index, &mut validation_report);
    validate_agent_dependency_cycles(workflow, &validation_index, &mut validation_report);

    validation_report
}

fn build_validation_index(workflow: &Workflow, validation_report: &mut ValidationReport) -> ValidationIndex {
    let mut validation_index = ValidationIndex::default();

    let mut has_input_declaration = false;
    let mut has_secrets_declaration = false;
    let mut has_output_declaration = false;

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_name = provider_declaration.name.clone();

                if validation_index.provider_infos.contains_key(&provider_name) {
                    validation_report.push_issue(ValidationIssue::DuplicateProvider { provider_name });

                    continue;
                }

                let provider_info = ProviderInfo {
                    declared_models: extract_declared_provider_models(provider_declaration.properties.as_slice()),
                };

                validation_index.provider_infos.insert(provider_name, provider_info);
            }
            Declaration::Schema(schema_declaration) => {
                let inserted_schema = validation_index.schema_names.insert(schema_declaration.name.clone());

                if !inserted_schema {
                    validation_report.push_issue(ValidationIssue::DuplicateSchema {
                        schema_name: schema_declaration.name.clone(),
                    });
                }
            }
            Declaration::Agent(agent_declaration) => {
                let inserted_agent = validation_index.agent_names.insert(agent_declaration.name.clone());

                if !inserted_agent {
                    validation_report.push_issue(ValidationIssue::DuplicateAgent {
                        agent_name: agent_declaration.name.clone(),
                    });
                }
            }
            Declaration::Input(_) => {
                if has_input_declaration {
                    validation_report.push_issue(ValidationIssue::DuplicateSingletonDeclaration {
                        declaration_kind: SingletonDeclarationKind::Input,
                    });
                }

                has_input_declaration = true;
            }
            Declaration::Secrets(_) => {
                if has_secrets_declaration {
                    validation_report.push_issue(ValidationIssue::DuplicateSingletonDeclaration {
                        declaration_kind: SingletonDeclarationKind::Secrets,
                    });
                }

                has_secrets_declaration = true;
            }
            Declaration::Output(_) => {
                if has_output_declaration {
                    validation_report.push_issue(ValidationIssue::DuplicateSingletonDeclaration {
                        declaration_kind: SingletonDeclarationKind::Output,
                    });
                }

                has_output_declaration = true;
            }
        }
    }

    validation_index
}

fn extract_declared_provider_models(provider_properties: &[ObjectField]) -> Option<HashSet<String>> {
    let models_property = provider_properties
        .iter()
        .find(|provider_property| provider_property.name == "models")?;

    let Expression::ArrayLiteral(models_expression_values) = &models_property.value else {
        return None;
    };

    let mut declared_models = HashSet::new();

    for model_expression in models_expression_values {
        let Expression::StringLiteral(model_name) = model_expression else {
            return None;
        };

        declared_models.insert(model_name.clone());
    }

    Some(declared_models)
}

fn validate_schema_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut unknown_schema_references = HashSet::new();

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Input(input_declaration) => {
                for typed_field in &input_declaration.fields {
                    validate_type_expression_for_schemas(
                        &typed_field.field_type,
                        ValidationContext::Input,
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                    );
                }
            }
            Declaration::Secrets(secrets_declaration) => {
                for typed_field in &secrets_declaration.fields {
                    validate_type_expression_for_schemas(
                        &typed_field.field_type,
                        ValidationContext::Secrets,
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                    );
                }
            }
            Declaration::Schema(schema_declaration) => {
                let schema_context = ValidationContext::Schema(schema_declaration.name.clone());

                for typed_field in &schema_declaration.fields {
                    validate_type_expression_for_schemas(
                        &typed_field.field_type,
                        schema_context.clone(),
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                for agent_property in &agent_declaration.properties {
                    if let AgentProperty::Output(output_type) = agent_property {
                        validate_type_expression_for_schemas(
                            output_type,
                            agent_context.clone(),
                            validation_index,
                            validation_report,
                            &mut unknown_schema_references,
                        );
                    }
                }
            }
            Declaration::Provider(_) | Declaration::Output(_) => {}
        }
    }
}

fn validate_type_expression_for_schemas(
    type_expression: &TypeExpression,
    context: ValidationContext,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
    unknown_schema_references: &mut HashSet<(ValidationContext, String)>,
) {
    match type_expression {
        TypeExpression::SchemaReference(referenced_schema_name) => {
            if validation_index.schema_names.contains(referenced_schema_name) {
                return;
            }

            let issue_key = (context.clone(), referenced_schema_name.clone());

            if unknown_schema_references.insert(issue_key) {
                validation_report.push_issue(ValidationIssue::UnknownSchemaReference {
                    referenced_schema: referenced_schema_name.clone(),
                    context,
                });
            }
        }
        TypeExpression::Array {
            item_type,
            fixed_length: _,
        } => {
            validate_type_expression_for_schemas(item_type, context, validation_index, validation_report, unknown_schema_references);
        }
        TypeExpression::Tuple(type_expressions) | TypeExpression::Union(type_expressions) => {
            for nested_type_expression in type_expressions {
                validate_type_expression_for_schemas(
                    nested_type_expression,
                    context.clone(),
                    validation_index,
                    validation_report,
                    unknown_schema_references,
                );
            }
        }
        TypeExpression::Object(object_fields) => {
            for object_field in object_fields {
                validate_type_expression_for_schemas(
                    &object_field.field_type,
                    context.clone(),
                    validation_index,
                    validation_report,
                    unknown_schema_references,
                );
            }
        }
        TypeExpression::String
        | TypeExpression::Number
        | TypeExpression::Float
        | TypeExpression::Boolean
        | TypeExpression::Null
        | TypeExpression::StringEnum(_) => {}
    }
}

fn validate_agent_model_bindings(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        for agent_property in &agent_declaration.properties {
            let AgentProperty::Model(model_expression) = agent_property else {
                continue;
            };

            validate_model_expression(&agent_declaration.name, model_expression, validation_index, validation_report);
        }
    }
}

fn validate_model_expression(
    agent_name: &str,
    model_expression: &Expression,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let Expression::FunctionCall(model_call) = model_expression else {
        validation_report.push_issue(ValidationIssue::InvalidModelExpression {
            agent_name: agent_name.to_owned(),
        });

        return;
    };

    if !model_call.callee.accesses.is_empty() {
        validation_report.push_issue(ValidationIssue::InvalidModelExpression {
            agent_name: agent_name.to_owned(),
        });

        return;
    }

    let provider_name = model_call.callee.root.clone();
    let Some(provider_info) = validation_index.provider_infos.get(&provider_name) else {
        validation_report.push_issue(ValidationIssue::UnknownProviderInModel {
            agent_name: agent_name.to_owned(),
            provider_name,
        });

        return;
    };

    let Some(model_name) = extract_model_name(model_call) else {
        validation_report.push_issue(ValidationIssue::InvalidModelExpression {
            agent_name: agent_name.to_owned(),
        });

        return;
    };

    let Some(declared_models) = &provider_info.declared_models else {
        return;
    };

    if declared_models.contains(&model_name) {
        return;
    }

    validation_report.push_issue(ValidationIssue::UnknownModelForProvider {
        agent_name: agent_name.to_owned(),
        provider_name,
        model_name,
    });
}

fn extract_model_name(model_call: &FunctionCall) -> Option<String> {
    for call_argument in &model_call.arguments {
        match call_argument {
            CallArgument::Positional(Expression::StringLiteral(model_name)) => {
                return Some(model_name.clone());
            }
            CallArgument::Named(named_argument) if named_argument.name == "model" => {
                let Expression::StringLiteral(model_name) = &named_argument.value else {
                    return None;
                };

                return Some(model_name.clone());
            }
            CallArgument::Named(_) | CallArgument::Positional(_) => {}
        }
    }

    None
}

fn validate_agent_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut unknown_agent_references = HashSet::new();

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                for provider_property in &provider_declaration.properties {
                    validate_expression_for_agent_references(
                        &provider_property.value,
                        provider_context.clone(),
                        validation_index,
                        validation_report,
                        &mut unknown_agent_references,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                if let Some(agent_for_loop) = &agent_declaration.for_loop {
                    validate_expression_for_agent_references(
                        &agent_for_loop.iterable,
                        agent_context.clone(),
                        validation_index,
                        validation_report,
                        &mut unknown_agent_references,
                    );
                }

                for agent_property in &agent_declaration.properties {
                    match agent_property {
                        AgentProperty::Model(model_expression)
                        | AgentProperty::Prompt(model_expression)
                        | AgentProperty::Context(model_expression)
                        | AgentProperty::Inference(model_expression)
                        | AgentProperty::Tools(model_expression)
                        | AgentProperty::Custom {
                            name: _,
                            value: model_expression,
                        } => {
                            validate_expression_for_agent_references(
                                model_expression,
                                agent_context.clone(),
                                validation_index,
                                validation_report,
                                &mut unknown_agent_references,
                            );
                        }
                        AgentProperty::Output(_) => {}
                    }
                }
            }
            Declaration::Output(output_declaration) => {
                for output_field in &output_declaration.fields {
                    validate_expression_for_agent_references(
                        &output_field.value,
                        ValidationContext::Output,
                        validation_index,
                        validation_report,
                        &mut unknown_agent_references,
                    );
                }
            }
            Declaration::Secrets(_) | Declaration::Input(_) | Declaration::Schema(_) => {}
        }
    }
}

fn validate_expression_for_agent_references(
    expression: &Expression,
    context: ValidationContext,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
    unknown_agent_references: &mut HashSet<(ValidationContext, String)>,
) {
    match expression {
        Expression::Reference(reference) => {
            validate_reference_for_agent(reference, context, validation_index, validation_report, unknown_agent_references);
        }
        Expression::FunctionCall(function_call) => {
            validate_reference_for_agent(
                &function_call.callee,
                context.clone(),
                validation_index,
                validation_report,
                unknown_agent_references,
            );

            for call_argument in &function_call.arguments {
                match call_argument {
                    CallArgument::Positional(argument_expression) => {
                        validate_expression_for_agent_references(
                            argument_expression,
                            context.clone(),
                            validation_index,
                            validation_report,
                            unknown_agent_references,
                        );
                    }
                    CallArgument::Named(named_argument) => {
                        validate_expression_for_agent_references(
                            &named_argument.value,
                            context.clone(),
                            validation_index,
                            validation_report,
                            unknown_agent_references,
                        );
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_values) => {
            for array_value in array_values {
                validate_expression_for_agent_references(
                    array_value,
                    context.clone(),
                    validation_index,
                    validation_report,
                    unknown_agent_references,
                );
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            for object_field in object_fields {
                validate_expression_for_agent_references(
                    &object_field.value,
                    context.clone(),
                    validation_index,
                    validation_report,
                    unknown_agent_references,
                );
            }
        }
        Expression::StringTemplate(string_template) => {
            for string_template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                    validate_expression_for_agent_references(
                        interpolation_expression,
                        context.clone(),
                        validation_index,
                        validation_report,
                        unknown_agent_references,
                    );
                }
            }
        }
        Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
    }
}

fn validate_reference_for_agent(
    reference: &Reference,
    context: ValidationContext,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
    unknown_agent_references: &mut HashSet<(ValidationContext, String)>,
) {
    if reference.root != "agent" {
        return;
    }

    let Some(first_access) = reference.accesses.first() else {
        return;
    };

    validate_agent_reference_name(
        first_access.field.as_str(),
        context,
        validation_index,
        validation_report,
        unknown_agent_references,
    );
}

fn validate_agent_reference_name(
    referenced_agent_name: &str,
    context: ValidationContext,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
    unknown_agent_references: &mut HashSet<(ValidationContext, String)>,
) {
    if validation_index.agent_names.contains(referenced_agent_name) {
        return;
    }

    let issue_key = (context.clone(), referenced_agent_name.to_owned());

    if unknown_agent_references.insert(issue_key) {
        validation_report.push_issue(ValidationIssue::UnknownAgentReference {
            referenced_agent: referenced_agent_name.to_owned(),
            context,
        });
    }
}

fn validate_agent_dependency_cycles(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut dependency_graph = DiGraph::<String, ()>::new();
    let mut node_index_by_agent_name = HashMap::<String, NodeIndex>::new();
    let mut sorted_agent_names: Vec<String> = validation_index.agent_names.iter().cloned().collect();

    sorted_agent_names.sort();

    for agent_name in &sorted_agent_names {
        let node_index = dependency_graph.add_node(agent_name.clone());
        node_index_by_agent_name.insert(agent_name.clone(), node_index);
    }

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let Some(source_agent_node) = node_index_by_agent_name.get(&agent_declaration.name).copied() else {
            continue;
        };

        let mut referenced_agents = HashSet::new();

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            collect_agent_dependencies_from_expression(&agent_for_loop.iterable, &mut referenced_agents);
        }

        for agent_property in &agent_declaration.properties {
            match agent_property {
                AgentProperty::Model(model_expression)
                | AgentProperty::Prompt(model_expression)
                | AgentProperty::Context(model_expression)
                | AgentProperty::Inference(model_expression)
                | AgentProperty::Tools(model_expression)
                | AgentProperty::Custom {
                    name: _,
                    value: model_expression,
                } => {
                    collect_agent_dependencies_from_expression(model_expression, &mut referenced_agents);
                }
                AgentProperty::Output(_) => {}
            }
        }

        for referenced_agent in referenced_agents {
            let Some(target_agent_node) = node_index_by_agent_name.get(&referenced_agent).copied() else {
                continue;
            };

            if dependency_graph.find_edge(source_agent_node, target_agent_node).is_none() {
                dependency_graph.add_edge(source_agent_node, target_agent_node, ());
            }
        }
    }

    for strongly_connected_component in kosaraju_scc(&dependency_graph) {
        let has_cycle = if strongly_connected_component.len() > 1 {
            true
        } else {
            let node_index = strongly_connected_component[0];
            dependency_graph.find_edge(node_index, node_index).is_some()
        };

        if !has_cycle {
            continue;
        }

        let mut cycle_agent_names: Vec<String> = strongly_connected_component
            .into_iter()
            .map(|node_index| dependency_graph[node_index].clone())
            .collect();

        cycle_agent_names.sort();

        validation_report.push_issue(ValidationIssue::AgentDependencyCycle {
            agent_names: cycle_agent_names,
        });
    }
}

fn collect_agent_dependencies_from_expression(expression: &Expression, referenced_agents: &mut HashSet<String>) {
    match expression {
        Expression::Reference(reference) => {
            collect_agent_dependency_from_reference(reference, referenced_agents);
        }
        Expression::FunctionCall(function_call) => {
            collect_agent_dependency_from_reference(&function_call.callee, referenced_agents);

            for call_argument in &function_call.arguments {
                match call_argument {
                    CallArgument::Positional(argument_expression) => {
                        collect_agent_dependencies_from_expression(argument_expression, referenced_agents);
                    }
                    CallArgument::Named(named_argument) => {
                        collect_agent_dependencies_from_expression(&named_argument.value, referenced_agents);
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_values) => {
            for array_value in array_values {
                collect_agent_dependencies_from_expression(array_value, referenced_agents);
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            for object_field in object_fields {
                collect_agent_dependencies_from_expression(&object_field.value, referenced_agents);
            }
        }
        Expression::StringTemplate(string_template) => {
            for string_template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                    collect_agent_dependencies_from_expression(interpolation_expression, referenced_agents);
                }
            }
        }
        Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
    }
}

fn collect_agent_dependency_from_reference(reference: &Reference, referenced_agents: &mut HashSet<String>) {
    if reference.root != "agent" {
        return;
    }

    let Some(first_access) = reference.accesses.first() else {
        return;
    };

    referenced_agents.insert(first_access.field.clone());
}

#[cfg(test)]
mod tests {
    use super::{validate_workflow, ValidationContext, ValidationIssue};
    use crate::dsl::macros::parse_inline_workflow;

    macro_rules! assert_has_validation_issue {
        ($validation_report:expr, $issue_pattern:pat $(if $guard:expr)? ) => {{
            assert!(
                $validation_report
                    .issues()
                    .iter()
                    .any(|validation_issue| matches!(validation_issue, $issue_pattern $(if $guard)?)),
                "expected matching validation issue; got {:?}",
                $validation_report.issues()
            );
        }};
    }

    macro_rules! assert_has_agent_cycle_issue {
        ($validation_report:expr, [$($agent_name:expr),+ $(,)?]) => {{
            let expected_agent_names = vec![$($agent_name.to_owned()),+];

            assert_has_validation_issue!(
                $validation_report,
                ValidationIssue::AgentDependencyCycle { agent_names }
                    if agent_names.len() == expected_agent_names.len()
                    && expected_agent_names
                        .iter()
                        .all(|expected_agent_name| agent_names.contains(expected_agent_name))
            );
        }};
    }

    #[test]
    fn reports_no_issues_for_valid_workflow() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            schema ResearchNote {
                summary: string
            }

            agent researcher {
                model: openai("gpt-4.1-mini")
                prompt: "Produce a short research note"
                output: schema.ResearchNote
            }

            output {
                note: agent.researcher
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert!(validation_report.is_valid());
        assert!(validation_report.issues().is_empty());
    }

    #[test]
    fn reports_unknown_provider_for_agent_model() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                model: missing_provider("gpt-4.1-mini")
                prompt: "Produce a short research note"
                output: string
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert_has_validation_issue!(
            validation_report,
            ValidationIssue::UnknownProviderInModel {
                agent_name,
                provider_name
            } if agent_name == "researcher" && provider_name == "missing_provider"
        );
    }

    #[test]
    fn reports_unknown_model_for_provider() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent researcher {
                model: openai("gpt-4.1")
                prompt: "Produce a short research note"
                output: string
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert_has_validation_issue!(
            validation_report,
            ValidationIssue::UnknownModelForProvider {
                agent_name,
                provider_name,
                model_name
            } if agent_name == "researcher" && provider_name == "openai" && model_name == "gpt-4.1"
        );
    }

    #[test]
    fn reports_unknown_agent_references() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent researcher {
                model: openai("gpt-4.1-mini")
                prompt: "Produce a short research note"
                output: string
            }

            output {
                note: agent.missing_agent
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert_has_validation_issue!(
            validation_report,
            ValidationIssue::UnknownAgentReference {
                referenced_agent,
                context
            } if referenced_agent == "missing_agent" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_unknown_schema_references() {
        let workflow = parse_inline_workflow! {
            schema Wrapper {
                payload: schema.MissingSchema
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert_has_validation_issue!(
            validation_report,
            ValidationIssue::UnknownSchemaReference {
                referenced_schema,
                context
            } if referenced_schema == "MissingSchema"
                && *context == ValidationContext::Schema("Wrapper".to_owned())
        );
    }

    #[test]
    fn reports_agent_dependency_cycles() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent alpha {
                model: openai("gpt-4.1-mini")
                prompt: agent.beta
                output: string
            }

            agent beta {
                model: openai("gpt-4.1-mini")
                prompt: agent.alpha
                output: string
            }
        };

        assert_has_agent_cycle_issue!(validate_workflow(&workflow), ["alpha", "beta"]);
    }

    #[test]
    fn reports_agent_dependency_cycles_from_interpolated_prompt_bindings() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent alpha {
                model: openai("gpt-4.1-mini")
                prompt: "Something {{ agent.beta }}"
                output: string
            }

            agent beta {
                model: openai("gpt-4.1-mini")
                prompt: "Something {{ agent.alpha }}"
                output: string
            }
        };

        assert_has_agent_cycle_issue!(validate_workflow(&workflow), ["alpha", "beta"]);
    }
}
