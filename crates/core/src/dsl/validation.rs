use super::ast::{
    AgentProperty, Declaration, Expression, FunctionCall, ModelCallArgumentName, ObjectField, Reference, ReferenceKeyword, SourceSpan,
    StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationReport {
    issues: Vec<ValidationIssue>,
    spans: Vec<Option<SourceSpan>>,
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

    pub fn issues_with_spans(&self) -> impl Iterator<Item = (&ValidationIssue, Option<SourceSpan>)> + '_ {
        self.issues.iter().zip(self.spans.iter().copied())
    }

    fn push_issue(&mut self, issue: ValidationIssue) {
        self.push_issue_with_span(issue, None);
    }

    fn push_issue_with_span(&mut self, issue: ValidationIssue, span: Option<SourceSpan>) {
        self.issues.push(issue);
        self.spans.push(span);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SingletonDeclarationKind {
    Secrets,
    Input,
    Output,
}

impl SingletonDeclarationKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Secrets => "secrets",
            Self::Input => "input",
            Self::Output => "output",
        }
    }
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

impl ValidationContext {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Provider(provider_name) => format!("provider `{provider_name}`"),
            Self::Schema(schema_name) => format!("schema `{schema_name}`"),
            Self::Agent(agent_name) => format!("agent `{agent_name}`"),
            Self::Input => "input declaration".to_string(),
            Self::Secrets => "secrets declaration".to_string(),
            Self::Output => "output declaration".to_string(),
        }
    }
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
    UnknownAgentProperty {
        agent_name: String,
        property_name: String,
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
    InvalidKeywordReferenceRoot {
        keyword: ReferenceKeyword,
        context: ValidationContext,
    },
    MissingInputDeclaration {
        context: ValidationContext,
    },
    MissingSecretsDeclaration {
        context: ValidationContext,
    },
    UnknownInputFieldReference {
        field_name: String,
        context: ValidationContext,
    },
    UnknownSecretsFieldReference {
        field_name: String,
        context: ValidationContext,
    },
    SecretReferenceInLlmContext {
        reference_path: String,
        context: ValidationContext,
    },
    MissingAgentOutputTypeForFieldReference {
        agent_name: String,
        context: ValidationContext,
    },
    InvalidReferencePath {
        reference_path: String,
        invalid_field: String,
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

impl ValidationIssue {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicateProvider { .. } => "duplicate_provider",
            Self::DuplicateSchema { .. } => "duplicate_schema",
            Self::DuplicateAgent { .. } => "duplicate_agent",
            Self::DuplicateSingletonDeclaration { .. } => "duplicate_singleton_declaration",
            Self::UnknownAgentProperty { .. } => "unknown_agent_property",
            Self::InvalidModelExpression { .. } => "invalid_model_expression",
            Self::UnknownProviderInModel { .. } => "unknown_provider_in_model",
            Self::UnknownModelForProvider { .. } => "unknown_model_for_provider",
            Self::UnknownAgentReference { .. } => "unknown_agent_reference",
            Self::InvalidKeywordReferenceRoot { .. } => "invalid_keyword_reference_root",
            Self::MissingInputDeclaration { .. } => "missing_input_declaration",
            Self::MissingSecretsDeclaration { .. } => "missing_secrets_declaration",
            Self::UnknownInputFieldReference { .. } => "unknown_input_field_reference",
            Self::UnknownSecretsFieldReference { .. } => "unknown_secrets_field_reference",
            Self::SecretReferenceInLlmContext { .. } => "secret_reference_in_llm_context",
            Self::MissingAgentOutputTypeForFieldReference { .. } => "missing_agent_output_type_for_field_reference",
            Self::InvalidReferencePath { .. } => "invalid_reference_path",
            Self::UnknownSchemaReference { .. } => "unknown_schema_reference",
            Self::AgentDependencyCycle { .. } => "agent_dependency_cycle",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::DuplicateProvider { provider_name } => {
                format!("Provider `{provider_name}` is declared more than once.")
            }
            Self::DuplicateSchema { schema_name } => {
                format!("Schema `{schema_name}` is declared more than once.")
            }
            Self::DuplicateAgent { agent_name } => {
                format!("Agent `{agent_name}` is declared more than once.")
            }
            Self::DuplicateSingletonDeclaration { declaration_kind } => {
                format!("`{}` declaration is defined more than once.", declaration_kind.as_str())
            }
            Self::UnknownAgentProperty { agent_name, property_name } => {
                format!("Agent `{agent_name}` declares unsupported property `{property_name}`.")
            }
            Self::InvalidModelExpression { agent_name } => {
                format!("Agent `{agent_name}` has an invalid `model` expression.")
            }
            Self::UnknownProviderInModel { agent_name, provider_name } => {
                format!("Agent `{agent_name}` references unknown provider `{provider_name}` in `model`.")
            }
            Self::UnknownModelForProvider {
                agent_name,
                provider_name,
                model_name,
            } => {
                format!("Agent `{agent_name}` uses model `{model_name}` which is not registered by provider `{provider_name}`.")
            }
            Self::UnknownAgentReference { referenced_agent, context } => {
                format!("Unknown agent `{referenced_agent}` referenced in {}.", context.describe())
            }
            Self::InvalidKeywordReferenceRoot { keyword, context } => {
                format!("`{}` reference requires a field path in {}.", keyword.as_str(), context.describe())
            }
            Self::MissingInputDeclaration { context } => {
                format!("Missing `input` declaration required by {}.", context.describe())
            }
            Self::MissingSecretsDeclaration { context } => {
                format!("Missing `secrets` declaration required by {}.", context.describe())
            }
            Self::UnknownInputFieldReference { field_name, context } => {
                format!("Unknown input field `{field_name}` referenced in {}.", context.describe())
            }
            Self::UnknownSecretsFieldReference { field_name, context } => {
                format!("Unknown secrets field `{field_name}` referenced in {}.", context.describe())
            }
            Self::SecretReferenceInLlmContext { reference_path, context } => {
                format!("Secret reference `{reference_path}` is not allowed in {}.", context.describe())
            }
            Self::MissingAgentOutputTypeForFieldReference { agent_name, context } => {
                format!(
                    "Agent `{agent_name}` must declare `output` before field access in {}.",
                    context.describe()
                )
            }
            Self::InvalidReferencePath {
                reference_path,
                invalid_field,
                context,
            } => {
                format!(
                    "Reference `{reference_path}` has no field `{invalid_field}` in {}.",
                    context.describe()
                )
            }
            Self::UnknownSchemaReference {
                referenced_schema,
                context,
            } => {
                format!("Unknown schema `schema.{referenced_schema}` referenced in {}.", context.describe())
            }
            Self::AgentDependencyCycle { agent_names } => {
                format!("Circular agent dependency detected: {}.", agent_names.join(", "))
            }
        }
    }
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
    schema_field_types: HashMap<String, HashMap<String, TypeExpression>>,
    input_field_types: Option<HashMap<String, TypeExpression>>,
    secrets_field_types: Option<HashMap<String, TypeExpression>>,
    agent_output_types: HashMap<String, Option<TypeExpression>>,
}

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut validation_report = ValidationReport::default();
    let validation_index = build_validation_index(workflow, &mut validation_report);

    validate_schema_references(workflow, &validation_index, &mut validation_report);
    validate_agent_properties(workflow, &mut validation_report);
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
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateProvider { provider_name },
                        Some(provider_declaration.span),
                    );

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
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateSchema {
                            schema_name: schema_declaration.name.clone(),
                        },
                        Some(schema_declaration.span),
                    );

                    continue;
                }

                let schema_field_types = collect_field_types(schema_declaration.fields.as_slice());
                validation_index
                    .schema_field_types
                    .insert(schema_declaration.name.clone(), schema_field_types);
            }
            Declaration::Agent(agent_declaration) => {
                let inserted_agent = validation_index.agent_names.insert(agent_declaration.name.clone());

                if !inserted_agent {
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateAgent {
                            agent_name: agent_declaration.name.clone(),
                        },
                        Some(agent_declaration.span),
                    );

                    continue;
                }

                let agent_output_type = extract_agent_output_type(agent_declaration.properties.as_slice());
                validation_index
                    .agent_output_types
                    .insert(agent_declaration.name.clone(), agent_output_type);
            }
            Declaration::Input(input_declaration) => {
                if has_input_declaration {
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateSingletonDeclaration {
                            declaration_kind: SingletonDeclarationKind::Input,
                        },
                        Some(input_declaration.span),
                    );
                }

                has_input_declaration = true;

                if validation_index.input_field_types.is_none() {
                    validation_index.input_field_types = Some(collect_field_types(input_declaration.fields.as_slice()));
                }
            }
            Declaration::Secrets(secrets_declaration) => {
                if has_secrets_declaration {
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateSingletonDeclaration {
                            declaration_kind: SingletonDeclarationKind::Secrets,
                        },
                        Some(secrets_declaration.span),
                    );
                }

                has_secrets_declaration = true;

                if validation_index.secrets_field_types.is_none() {
                    validation_index.secrets_field_types = Some(collect_field_types(secrets_declaration.fields.as_slice()));
                }
            }
            Declaration::Output(output_declaration) => {
                if has_output_declaration {
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateSingletonDeclaration {
                            declaration_kind: SingletonDeclarationKind::Output,
                        },
                        Some(output_declaration.span),
                    );
                }

                has_output_declaration = true;
            }
        }
    }

    validation_index
}

fn collect_field_types(typed_fields: &[TypedField]) -> HashMap<String, TypeExpression> {
    typed_fields
        .iter()
        .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
        .collect()
}

fn extract_agent_output_type(agent_properties: &[AgentProperty]) -> Option<TypeExpression> {
    agent_properties.iter().find_map(|agent_property| {
        if let AgentProperty::Output(type_expression) = agent_property {
            Some(type_expression.clone())
        } else {
            None
        }
    })
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

fn validate_agent_properties(workflow: &Workflow, validation_report: &mut ValidationReport) {
    let mut unknown_agent_properties = HashSet::<(String, String)>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        for agent_property in &agent_declaration.properties {
            let AgentProperty::Custom {
                name: property_name,
                value: _,
            } = agent_property
            else {
                continue;
            };

            let issue_key = (agent_declaration.name.clone(), property_name.clone());

            if !unknown_agent_properties.insert(issue_key.clone()) {
                continue;
            }

            validation_report.push_issue_with_span(
                ValidationIssue::UnknownAgentProperty {
                    agent_name: issue_key.0,
                    property_name: issue_key.1,
                },
                Some(agent_declaration.span),
            );
        }
    }
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
                        Some(typed_field.span),
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
                        Some(typed_field.span),
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
                        Some(typed_field.span),
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
                            Some(agent_declaration.span),
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
    span: Option<SourceSpan>,
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
                validation_report.push_issue_with_span(
                    ValidationIssue::UnknownSchemaReference {
                        referenced_schema: referenced_schema_name.clone(),
                        context,
                    },
                    span,
                );
            }
        }
        TypeExpression::Array {
            item_type,
            fixed_length: _,
        } => {
            validate_type_expression_for_schemas(
                item_type,
                context,
                span,
                validation_index,
                validation_report,
                unknown_schema_references,
            );
        }
        TypeExpression::Tuple(type_expressions) | TypeExpression::Union(type_expressions) => {
            for nested_type_expression in type_expressions {
                validate_type_expression_for_schemas(
                    nested_type_expression,
                    context.clone(),
                    span,
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
                    span,
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

            validate_model_expression(
                &agent_declaration.name,
                model_expression,
                Some(agent_declaration.span),
                validation_index,
                validation_report,
            );
        }
    }
}

fn validate_model_expression(
    agent_name: &str,
    model_expression: &Expression,
    declaration_span: Option<SourceSpan>,
    validation_index: &ValidationIndex,
    validation_report: &mut ValidationReport,
) {
    let Expression::FunctionCall(model_call) = model_expression else {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            declaration_span,
        );

        return;
    };

    let model_span = Some(model_call.callee.span);

    if !model_call.callee.accesses.is_empty() {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            model_span,
        );

        return;
    }

    if model_call.callee.root.as_identifier().is_none() {
        let provider_root_keyword = model_call
            .callee
            .root
            .keyword()
            .expect("non-identifier reference root should be a keyword");

        validation_report.push_issue_with_span(
            ValidationIssue::UnknownProviderInModel {
                agent_name: agent_name.to_owned(),
                provider_name: provider_root_keyword.as_str().to_owned(),
            },
            model_span,
        );

        return;
    }

    let provider_name = model_call
        .callee
        .root
        .as_identifier()
        .expect("provider root should be identifier after early return")
        .to_owned();

    let Some(provider_info) = validation_index.provider_infos.get(&provider_name) else {
        validation_report.push_issue_with_span(
            ValidationIssue::UnknownProviderInModel {
                agent_name: agent_name.to_owned(),
                provider_name,
            },
            model_span,
        );

        return;
    };

    let Some(model_name) = extract_model_name(model_call) else {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            model_span,
        );

        return;
    };

    let Some(declared_models) = &provider_info.declared_models else {
        return;
    };

    if declared_models.contains(&model_name) {
        return;
    }

    validation_report.push_issue_with_span(
        ValidationIssue::UnknownModelForProvider {
            agent_name: agent_name.to_owned(),
            provider_name,
            model_name,
        },
        model_span,
    );
}

fn extract_model_name(model_call: &FunctionCall) -> Option<String> {
    for call_argument in &model_call.arguments {
        if call_argument.named_argument_name().is_none() {
            if let Expression::StringLiteral(model_name) = call_argument.expression() {
                return Some(model_name.clone());
            }

            continue;
        }

        if call_argument.named_argument_name() != Some(ModelCallArgumentName::Model.as_str()) {
            continue;
        }

        let Expression::StringLiteral(model_name) = call_argument.expression() else {
            return None;
        };

        return Some(model_name.clone());
    }

    None
}

fn validate_agent_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut keyword_reference_validation_state = KeywordReferenceValidationState::new(validation_index, validation_report);

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                for provider_property in &provider_declaration.properties {
                    keyword_reference_validation_state.validate_expression(
                        &provider_property.value,
                        provider_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                if let Some(agent_for_loop) = &agent_declaration.for_loop {
                    keyword_reference_validation_state.validate_expression(
                        &agent_for_loop.iterable,
                        agent_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }

                for agent_property in &agent_declaration.properties {
                    match agent_property {
                        AgentProperty::Prompt(model_expression) | AgentProperty::Context(model_expression) => {
                            keyword_reference_validation_state.validate_expression(
                                model_expression,
                                agent_context.clone(),
                                SecretReferencePolicy::Forbid,
                            );
                        }
                        AgentProperty::Model(model_expression)
                        | AgentProperty::Inference(model_expression)
                        | AgentProperty::Tools(model_expression)
                        | AgentProperty::Custom {
                            name: _,
                            value: model_expression,
                        } => {
                            keyword_reference_validation_state.validate_expression(
                                model_expression,
                                agent_context.clone(),
                                SecretReferencePolicy::Allow,
                            );
                        }
                        AgentProperty::Output(_) => {}
                    }
                }
            }
            Declaration::Output(output_declaration) => {
                for output_field in &output_declaration.fields {
                    keyword_reference_validation_state.validate_expression(
                        &output_field.value,
                        ValidationContext::Output,
                        SecretReferencePolicy::Forbid,
                    );
                }
            }
            Declaration::Secrets(_) | Declaration::Input(_) | Declaration::Schema(_) => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretReferencePolicy {
    Allow,
    Forbid,
}

struct KeywordReferenceValidationState<'validation> {
    validation_index: &'validation ValidationIndex,
    validation_report: &'validation mut ValidationReport,
    unknown_agent_references: HashSet<(ValidationContext, String)>,
    invalid_keyword_reference_roots: HashSet<(ValidationContext, ReferenceKeyword)>,
    secret_reference_leaks: HashSet<(ValidationContext, String)>,
    missing_agent_output_type_references: HashSet<(ValidationContext, String)>,
    invalid_reference_paths: HashSet<(ValidationContext, String, String)>,
    missing_input_declaration_contexts: HashSet<ValidationContext>,
    missing_secrets_declaration_contexts: HashSet<ValidationContext>,
    unknown_input_field_references: HashSet<(ValidationContext, String)>,
    unknown_secrets_field_references: HashSet<(ValidationContext, String)>,
}

impl<'validation> KeywordReferenceValidationState<'validation> {
    fn new(validation_index: &'validation ValidationIndex, validation_report: &'validation mut ValidationReport) -> Self {
        Self {
            validation_index,
            validation_report,
            unknown_agent_references: HashSet::new(),
            invalid_keyword_reference_roots: HashSet::new(),
            secret_reference_leaks: HashSet::new(),
            missing_agent_output_type_references: HashSet::new(),
            invalid_reference_paths: HashSet::new(),
            missing_input_declaration_contexts: HashSet::new(),
            missing_secrets_declaration_contexts: HashSet::new(),
            unknown_input_field_references: HashSet::new(),
            unknown_secrets_field_references: HashSet::new(),
        }
    }

    fn validate_expression(&mut self, expression: &Expression, context: ValidationContext, secret_reference_policy: SecretReferencePolicy) {
        match expression {
            Expression::Reference(reference) => {
                self.validate_reference(reference, context, secret_reference_policy);
            }
            Expression::FunctionCall(function_call) => {
                self.validate_reference(&function_call.callee, context.clone(), secret_reference_policy);

                for call_argument in &function_call.arguments {
                    self.validate_expression(call_argument.expression(), context.clone(), secret_reference_policy);
                }
            }
            Expression::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    self.validate_expression(array_value, context.clone(), secret_reference_policy);
                }
            }
            Expression::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    self.validate_expression(&object_field.value, context.clone(), secret_reference_policy);
                }
            }
            Expression::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        self.validate_expression(interpolation_expression, context.clone(), secret_reference_policy);
                    }
                }
            }
            Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
        }
    }

    fn validate_reference(&mut self, reference: &Reference, context: ValidationContext, secret_reference_policy: SecretReferencePolicy) {
        let Some(reference_root_keyword) = reference.root_keyword() else {
            return;
        };

        if reference_root_keyword == ReferenceKeyword::Secrets && secret_reference_policy == SecretReferencePolicy::Forbid {
            self.push_secret_reference_leak(reference, context.clone());
        }

        let Some(_) = reference.first_access() else {
            let issue_key = (context.clone(), reference_root_keyword);

            if self.invalid_keyword_reference_roots.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::InvalidKeywordReferenceRoot {
                        keyword: reference_root_keyword,
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        match reference_root_keyword {
            ReferenceKeyword::Agent => {
                self.validate_agent_reference(reference, context);
            }
            ReferenceKeyword::Input => {
                self.validate_input_reference(reference, context);
            }
            ReferenceKeyword::Secrets => {
                self.validate_secrets_reference(reference, context);
            }
            ReferenceKeyword::Tool => {}
        }
    }

    fn validate_agent_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_agent_name = reference
            .accesses
            .first()
            .expect("agent reference should include first field after root")
            .field
            .as_str();

        if !self.validate_agent_reference_name(referenced_agent_name, context.clone(), Some(reference.span)) {
            return;
        }

        if reference.accesses.len() == 1 {
            return;
        }

        let Some(agent_output_type) = self
            .validation_index
            .agent_output_types
            .get(referenced_agent_name)
            .and_then(Clone::clone)
        else {
            let issue_key = (context.clone(), referenced_agent_name.to_owned());

            if self.missing_agent_output_type_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::MissingAgentOutputTypeForFieldReference {
                        agent_name: referenced_agent_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        self.validate_reference_path(reference, 1, agent_output_type, context);
    }

    fn validate_input_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("input reference should include first field after root")
            .field
            .as_str();

        let Some(input_field_types) = self.validation_index.input_field_types.as_ref() else {
            if self.missing_input_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(ValidationIssue::MissingInputDeclaration { context }, Some(reference.span));
            }

            return;
        };

        let Some(input_field_type) = input_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_input_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownInputFieldReference {
                        field_name: referenced_field_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.accesses.len() == 1 {
            return;
        }

        self.validate_reference_path(reference, 1, input_field_type.clone(), context);
    }

    fn validate_secrets_reference(&mut self, reference: &Reference, context: ValidationContext) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("secrets reference should include first field after root")
            .field
            .as_str();

        let Some(secrets_field_types) = self.validation_index.secrets_field_types.as_ref() else {
            if self.missing_secrets_declaration_contexts.insert(context.clone()) {
                self.validation_report
                    .push_issue_with_span(ValidationIssue::MissingSecretsDeclaration { context }, Some(reference.span));
            }

            return;
        };

        let Some(secrets_field_type) = secrets_field_types.get(referenced_field_name) else {
            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_secrets_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownSecretsFieldReference {
                        field_name: referenced_field_name.to_owned(),
                        context,
                    },
                    Some(reference.span),
                );
            }

            return;
        };

        if reference.accesses.len() == 1 {
            return;
        }

        self.validate_reference_path(reference, 1, secrets_field_type.clone(), context);
    }

    fn validate_reference_path(
        &mut self,
        reference: &Reference,
        path_start_index: usize,
        start_type: TypeExpression,
        context: ValidationContext,
    ) {
        let mut candidate_types = vec![start_type];

        for reference_access in reference.accesses.iter().skip(path_start_index) {
            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                self.collect_next_types_for_field(candidate_type, reference_access.field.as_str(), &mut next_candidate_types);
            }

            if next_candidate_types.is_empty() {
                let reference_path = self.reference_to_string(reference);
                let issue_key = (context.clone(), reference_path.clone(), reference_access.field.clone());

                if self.invalid_reference_paths.insert(issue_key) {
                    self.validation_report.push_issue_with_span(
                        ValidationIssue::InvalidReferencePath {
                            reference_path,
                            invalid_field: reference_access.field.clone(),
                            context,
                        },
                        Some(reference.span),
                    );
                }

                return;
            }

            candidate_types = next_candidate_types;
        }
    }

    fn collect_next_types_for_field(
        &self,
        candidate_type: &TypeExpression,
        field_name: &str,
        next_candidate_types: &mut Vec<TypeExpression>,
    ) {
        match candidate_type {
            TypeExpression::Object(typed_fields) => {
                if let Some(typed_field) = typed_fields.iter().find(|typed_field| typed_field.name == field_name) {
                    next_candidate_types.push(typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                let Some(schema_field_types) = self.validation_index.schema_field_types.get(schema_name) else {
                    return;
                };

                if let Some(field_type) = schema_field_types.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            TypeExpression::Union(type_expressions) => {
                for type_expression in type_expressions {
                    self.collect_next_types_for_field(type_expression, field_name, next_candidate_types);
                }
            }
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::StringEnum(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => {}
        }
    }

    fn reference_to_string(&self, reference: &Reference) -> String {
        reference.render_path()
    }

    fn push_secret_reference_leak(&mut self, reference: &Reference, context: ValidationContext) {
        let reference_path = self.reference_to_string(reference);
        let issue_key = (context.clone(), reference_path.clone());

        if self.secret_reference_leaks.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::SecretReferenceInLlmContext { reference_path, context },
                Some(reference.span),
            );
        }
    }

    fn validate_agent_reference_name(&mut self, referenced_agent_name: &str, context: ValidationContext, span: Option<SourceSpan>) -> bool {
        if self.validation_index.agent_names.contains(referenced_agent_name) {
            return true;
        }

        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.unknown_agent_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::UnknownAgentReference {
                    referenced_agent: referenced_agent_name.to_owned(),
                    context,
                },
                span,
            );
        }

        false
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
                collect_agent_dependencies_from_expression(call_argument.expression(), referenced_agents);
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
    if !reference.is_agent_root() {
        return;
    }

    let Some(agent_name) = reference.first_access_field() else {
        return;
    };

    referenced_agents.insert(agent_name.to_string());
}

#[cfg(test)]
mod tests {
    use super::{validate_workflow, ReferenceKeyword, SingletonDeclarationKind, ValidationContext, ValidationIssue};
    use crate::dsl::macros::parse_inline_workflow;
    use crate::dsl::parse_workflow;

    macro_rules! assert_issues_contain {
        ($validation_issues:expr, $issue_pattern:pat $(if $guard:expr)? ) => {{
            assert!(
                $validation_issues
                    .iter()
                    .any(|validation_issue| matches!(validation_issue, $issue_pattern $(if $guard)?)),
                "expected matching validation issue; got {:?}",
                $validation_issues
            );
        }};
    }

    macro_rules! assert_workflow_issues_contain {
        ($workflow:expr, $($issue_pattern:pat $(if $guard:expr)?),+ $(,)?) => {{
            let validation_report = validate_workflow(&$workflow);
            let validation_issues = validation_report.issues();

            $(
                assert_issues_contain!(validation_issues, $issue_pattern $(if $guard)?);
            )+
        }};
    }

    macro_rules! assert_workflow_issues_do_not_contain {
        ($workflow:expr, $issue_pattern:pat $(if $guard:expr)? ) => {{
            let validation_report = validate_workflow(&$workflow);
            let validation_issues = validation_report.issues();

            assert!(
                !validation_issues
                    .iter()
                    .any(|validation_issue| matches!(validation_issue, $issue_pattern $(if $guard)?)),
                "did not expect matching validation issue; got {:?}",
                validation_issues
            );
        }};
    }

    #[test]
    fn reports_no_issues_for_valid_workflow() {
        let workflow = parse_inline_workflow! {
            input {
                title: string
            }

            agent researcher {
                prompt: input.title
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
    fn reports_duplicate_named_resource_names() {
        let workflow = parse_inline_workflow! {
            provider openai { driver: "openai" }
            provider openai { driver: "anthropic" }

            schema User { name: string }
            schema User { id: string }

            agent researcher {}
            agent researcher {}
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProvider { provider_name } if provider_name == "openai",
            ValidationIssue::DuplicateSchema { schema_name } if schema_name == "User",
            ValidationIssue::DuplicateAgent { agent_name } if agent_name == "researcher"
        );
    }

    #[test]
    fn duplicate_schema_diagnostics_include_declaration_span() {
        let workflow_source = "schema User { name: string }\nschema User { id: string }\n";
        let workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let validation_report = validate_workflow(&workflow);

        let duplicate_schema_span = validation_report
            .issues_with_spans()
            .find_map(|(validation_issue, issue_span)| match validation_issue {
                ValidationIssue::DuplicateSchema { schema_name } if schema_name == "User" => issue_span,
                _ => None,
            })
            .expect("duplicate schema diagnostics should include span");

        assert_eq!(duplicate_schema_span.start.line, 2);
        assert_eq!(duplicate_schema_span.start.column, 1);
    }

    #[test]
    fn reports_duplicate_singleton_declarations() {
        let workflow = parse_inline_workflow! {
            input {}
            input {}

            secrets {}
            secrets {}

            output {}
            output {}
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Input,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Secrets,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Output
        );
    }

    #[test]
    fn reports_invalid_model_expression() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                model: "gpt-4.1-mini"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidModelExpression { agent_name } if agent_name == "researcher"
        );
    }

    #[test]
    fn reports_unknown_agent_properties() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent researcher {
                model: openai("gpt-4.1-mini")
                prompt: "Analyze this"
                retries: 3
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownAgentProperty {
                agent_name,
                property_name
            } if agent_name == "researcher" && property_name == "retries"
        );
    }

    #[test]
    fn exposes_stable_codes_and_messages_for_validation_issues() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "timeout".to_string(),
        };

        assert_eq!(issue.code(), "unknown_agent_property");
        assert!(issue.message().contains("unsupported property `timeout`"));
    }

    #[test]
    fn reports_secret_reference_leak_in_workflow_output() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            output {
                leaked: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::SecretReferenceInLlmContext { reference_path, context }
                if reference_path == "secrets.api_key" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_secret_reference_leak_in_prompt_interpolation() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            agent researcher {
                prompt: "Use token {{ secrets.api_key }}"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::SecretReferenceInLlmContext { reference_path, context }
                if reference_path == "secrets.api_key"
                    && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn allows_secret_reference_in_provider_configuration() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            provider openai {
                driver: "openai"
                api_key: secrets.api_key
                models: ["gpt-4.1-mini"]
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::SecretReferenceInLlmContext { .. });
    }

    #[test]
    fn reports_missing_input_declaration_for_input_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                prompt: input.topic
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingInputDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_unknown_input_field_reference() {
        let workflow = parse_inline_workflow! {
            input {
                title: string
            }

            agent researcher {
                prompt: input.topic
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownInputFieldReference { field_name, context }
                if field_name == "topic" && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reference_diagnostics_include_reference_span() {
        let workflow_source = "input {\n    title: string\n}\n\nagent researcher {\n    prompt: input.topic\n}\n";
        let workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let validation_report = validate_workflow(&workflow);

        let unknown_input_field_span = validation_report
            .issues_with_spans()
            .find_map(|(validation_issue, issue_span)| match validation_issue {
                ValidationIssue::UnknownInputFieldReference { field_name, .. } if field_name == "topic" => issue_span,
                _ => None,
            })
            .expect("unknown input field diagnostics should include span");

        assert_eq!(unknown_input_field_span.start.line, 6);
        assert_eq!(unknown_input_field_span.start.column, 13);
    }

    #[test]
    fn reports_invalid_nested_input_field_reference_path() {
        let workflow = parse_inline_workflow! {
            input {
                profile: {
                    name: string
                }
            }

            agent researcher {
                prompt: input.profile.age
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "input.profile.age"
                && invalid_field == "age"
                && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_missing_secrets_declaration_for_secrets_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                prompt: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingSecretsDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_unknown_secrets_field_reference() {
        let workflow = parse_inline_workflow! {
            secrets {
                openai_key: string
            }

            agent researcher {
                prompt: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownSecretsFieldReference { field_name, context }
                if field_name == "api_key" && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_invalid_nested_secrets_field_reference_path() {
        let workflow = parse_inline_workflow! {
            secrets {
                credentials: {
                    token: string
                }
            }

            agent researcher {
                prompt: secrets.credentials.key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "secrets.credentials.key"
                && invalid_field == "key"
                && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_invalid_bare_keyword_root_references() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                prompt: input
            }

            agent tooling {
                tools: [tool]
            }

            output {
                final: agent
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Input
                    && *context == ValidationContext::Agent("researcher".to_owned()),
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Tool
                    && *context == ValidationContext::Agent("tooling".to_owned()),
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Agent && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_unknown_provider_for_agent_model() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                model: missing_provider("gpt-4.1-mini")
            }
        };

        assert_workflow_issues_contain!(
            workflow,
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
            }
        };

        assert_workflow_issues_contain!(
            workflow,
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
            output {
                note: agent.missing_agent
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownAgentReference {
                referenced_agent,
                context
            } if referenced_agent == "missing_agent" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_missing_agent_output_type_for_nested_agent_field_reference() {
        let workflow = parse_inline_workflow! {
            agent producer {
                prompt: "produce"
            }

            agent consumer {
                prompt: agent.producer.summary
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name,
                context
            } if agent_name == "producer" && *context == ValidationContext::Agent("consumer".to_owned())
        );
    }

    #[test]
    fn reports_invalid_nested_agent_output_reference_path() {
        let workflow = parse_inline_workflow! {
            agent producer {
                output: {
                    summary: string
                }
            }

            output {
                result: agent.producer.score
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "agent.producer.score"
                && invalid_field == "score"
                && *context == ValidationContext::Output
        );
    }

    #[test]
    fn accepts_valid_nested_agent_output_reference_path() {
        let workflow = parse_inline_workflow! {
            schema Report {
                payload: {
                    score: number
                }
            }

            agent producer {
                output: schema.Report
            }

            output {
                result: agent.producer.payload.score
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert!(!validation_report
            .issues()
            .iter()
            .any(|validation_issue| matches!(validation_issue, ValidationIssue::InvalidReferencePath { .. })));
    }

    #[test]
    fn reports_unknown_schema_references() {
        let workflow = parse_inline_workflow! {
            schema Wrapper {
                payload: schema.MissingSchema
            }
        };

        assert_workflow_issues_contain!(
            workflow,
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
            agent alpha {
                prompt: agent.beta
            }

            agent beta {
                prompt: agent.alpha
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::AgentDependencyCycle { agent_names }
                if agent_names.len() == 2
                    && agent_names.contains(&"alpha".to_owned())
                    && agent_names.contains(&"beta".to_owned())
        );
    }

    #[test]
    fn reports_agent_dependency_cycles_from_interpolated_prompt_bindings() {
        let workflow = parse_inline_workflow! {
            agent alpha {
                prompt: "Something {{ agent.beta }}"
            }

            agent beta {
                prompt: "Something {{ agent.alpha }}"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::AgentDependencyCycle { agent_names }
                if agent_names.len() == 2
                    && agent_names.contains(&"alpha".to_owned())
                    && agent_names.contains(&"beta".to_owned())
        );
    }
}
