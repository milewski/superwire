use super::ast::{
    AgentDeclaration, AgentForLoop, AgentProperty, AgentPropertyName, Declaration, Expression, FunctionCall, ObjectField, Reference,
    ReferenceKeyword, SourceSpan, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::runtime::types::workflow_type_from_dsl;
use crate::runtime::InferenceSetting;
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

    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.issues_with_spans()
            .map(|(validation_issue, primary_span)| validation_issue.diagnostic(primary_span))
            .collect()
    }

    #[must_use]
    pub fn render(&self) -> String {
        self.diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.render())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[must_use]
    pub fn render_with_source(&self, source_text: &str, source_name: &str) -> String {
        self.diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.render_with_source(source_text, source_name))
            .collect::<Vec<_>>()
            .join("\n\n")
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
    Tool(String),
    Agent(String),
    Dynamic,
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
            Self::Tool(tool_name) => format!("tool `{tool_name}`"),
            Self::Agent(agent_name) => format!("agent `{agent_name}`"),
            Self::Dynamic => "dynamic declaration".to_string(),
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
    DuplicateTool {
        tool_name: String,
    },
    DuplicateAgent {
        agent_name: String,
    },
    DuplicateSingletonDeclaration {
        declaration_kind: SingletonDeclarationKind,
    },
    DuplicateProperty {
        property_name: String,
        context: ValidationContext,
    },
    UnknownAgentProperty {
        agent_name: String,
        property_name: String,
    },
    InvalidInferenceSettingValueType {
        agent_name: String,
        inference_setting: InferenceSetting,
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
    MissingDynamicDeclaration {
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
    UnknownDynamicFieldReference {
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
    MissingOptionalReferenceAccess {
        reference_path: String,
        field_name: String,
        context: ValidationContext,
    },
    InvalidReferencePath {
        reference_path: String,
        invalid_field: String,
        context: ValidationContext,
    },
    InvalidForLoopIterableType {
        agent_name: String,
        found_type: String,
    },
    UnknownSchemaReference {
        referenced_schema: String,
        context: ValidationContext,
    },
    UnknownToolReference {
        tool_name: String,
        agent_name: String,
    },
    InvalidTypeExpressionReference {
        reference_path: String,
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
            Self::DuplicateTool { .. } => "duplicate_tool",
            Self::DuplicateAgent { .. } => "duplicate_agent",
            Self::DuplicateSingletonDeclaration { .. } => "duplicate_singleton_declaration",
            Self::DuplicateProperty { .. } => "duplicate_property",
            Self::UnknownAgentProperty { .. } => "unknown_agent_property",
            Self::InvalidInferenceSettingValueType { .. } => "invalid_inference_setting_value_type",
            Self::InvalidModelExpression { .. } => "invalid_model_expression",
            Self::UnknownProviderInModel { .. } => "unknown_provider_in_model",
            Self::UnknownModelForProvider { .. } => "unknown_model_for_provider",
            Self::UnknownAgentReference { .. } => "unknown_agent_reference",
            Self::InvalidKeywordReferenceRoot { .. } => "invalid_keyword_reference_root",
            Self::MissingDynamicDeclaration { .. } => "missing_dynamic_declaration",
            Self::MissingInputDeclaration { .. } => "missing_input_declaration",
            Self::MissingSecretsDeclaration { .. } => "missing_secrets_declaration",
            Self::UnknownInputFieldReference { .. } => "unknown_input_field_reference",
            Self::UnknownDynamicFieldReference { .. } => "unknown_dynamic_field_reference",
            Self::UnknownSecretsFieldReference { .. } => "unknown_secrets_field_reference",
            Self::SecretReferenceInLlmContext { .. } => "secret_reference_in_llm_context",
            Self::MissingAgentOutputTypeForFieldReference { .. } => "missing_agent_output_type_for_field_reference",
            Self::MissingOptionalReferenceAccess { .. } => "missing_optional_reference_access",
            Self::InvalidReferencePath { .. } => "invalid_reference_path",
            Self::InvalidForLoopIterableType { .. } => "invalid_for_loop_iterable_type",
            Self::UnknownSchemaReference { .. } => "unknown_schema_reference",
            Self::UnknownToolReference { .. } => "unknown_tool_reference",
            Self::InvalidTypeExpressionReference { .. } => "invalid_type_expression_reference",
            Self::AgentDependencyCycle { .. } => "agent_dependency_cycle",
        }
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn message(&self) -> String {
        match self {
            Self::DuplicateProvider { provider_name } => {
                format!("Provider `{provider_name}` is declared more than once.")
            }
            Self::DuplicateSchema { schema_name } => {
                format!("Schema `{schema_name}` is declared more than once.")
            }
            Self::DuplicateTool { tool_name } => {
                format!("Tool `{tool_name}` is declared more than once.")
            }
            Self::DuplicateAgent { agent_name } => {
                format!("Agent `{agent_name}` is declared more than once.")
            }
            Self::DuplicateSingletonDeclaration { declaration_kind } => {
                format!("`{}` declaration is defined more than once.", declaration_kind.as_str())
            }
            Self::DuplicateProperty { property_name, context } => {
                format!("Property `{property_name}` is defined more than once in {}.", context.describe())
            }
            Self::UnknownAgentProperty { agent_name, property_name } => {
                format!("Agent `{agent_name}` declares unsupported property `{property_name}`.")
            }
            Self::InvalidInferenceSettingValueType {
                agent_name,
                inference_setting,
            } => {
                format!(
                    "Agent `{agent_name}` inference setting `{}` must be {}.",
                    inference_setting.key(),
                    inference_setting.expected_value_description()
                )
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
            Self::UnknownToolReference { tool_name, agent_name } => {
                format!("Agent `{agent_name}` references undeclared tool `tool.{tool_name}`.")
            }
            Self::InvalidKeywordReferenceRoot { keyword, context } => {
                format!("`{}` reference requires a field path in {}.", keyword.as_str(), context.describe())
            }
            Self::MissingDynamicDeclaration { context } => {
                format!("Missing `dynamic` declaration required by {}.", context.describe())
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
            Self::UnknownDynamicFieldReference { field_name, context } => {
                format!("Unknown dynamic field `{field_name}` referenced in {}.", context.describe())
            }
            Self::UnknownSecretsFieldReference { field_name, context } => {
                format!("Unknown secrets field `{field_name}` referenced in {}.", context.describe())
            }
            Self::SecretReferenceInLlmContext { reference_path, context } => {
                format!("Secret reference `{reference_path}` is not allowed in {}.", context.describe())
            }
            Self::MissingAgentOutputTypeForFieldReference { agent_name, context } => {
                format!(
                    "Agent `{agent_name}` must declare `output` before it can be referenced in {}.",
                    context.describe()
                )
            }
            Self::MissingOptionalReferenceAccess {
                reference_path,
                field_name,
                context,
            } => {
                format!(
                    "Reference `{reference_path}` must use `?.{field_name}` in {} because the path can be `null`.",
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
            Self::InvalidForLoopIterableType { agent_name, found_type } => {
                format!("Agent `{agent_name}` for-loop iterable must evaluate to an array, found `{found_type}`.")
            }
            Self::UnknownSchemaReference {
                referenced_schema,
                context,
            } => {
                format!("Unknown schema `schema.{referenced_schema}` referenced in {}.", context.describe())
            }
            Self::InvalidTypeExpressionReference { reference_path, context } => {
                format!(
                    "Type reference `{reference_path}` in {} must start with `agent.` or `input.`.",
                    context.describe()
                )
            }
            Self::AgentDependencyCycle { agent_names } => {
                format!("Circular agent dependency detected: {}.", agent_names.join(", "))
            }
        }
    }

    #[must_use]
    pub fn diagnostic(&self, primary_span: Option<SourceSpan>) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::from(self), DiagnosticSeverity::Error, self.message(), primary_span);

        if let Some(help_message) = self.help_message() {
            diagnostic = diagnostic.with_help(help_message);
        }

        diagnostic
    }

    #[must_use]
    fn help_message(&self) -> Option<String> {
        match self {
            Self::DuplicateProvider { .. }
            | Self::DuplicateSchema { .. }
            | Self::DuplicateTool { .. }
            | Self::DuplicateAgent { .. }
            | Self::DuplicateSingletonDeclaration { .. }
            | Self::DuplicateProperty { .. } => Some(self.duplicate_declaration_help_message()),
            Self::UnknownAgentProperty {
                agent_name: _,
                property_name,
            } => Some(Self::unknown_agent_property_help(property_name)),
            Self::InvalidInferenceSettingValueType {
                agent_name: _,
                inference_setting: _,
            }
            | Self::InvalidModelExpression { agent_name: _ }
            | Self::UnknownProviderInModel {
                agent_name: _,
                provider_name: _,
            }
            | Self::UnknownModelForProvider {
                agent_name: _,
                provider_name: _,
                model_name: _,
            } => Some(self.agent_model_help_message()),
            Self::UnknownAgentReference {
                referenced_agent: _,
                context: _,
            }
            | Self::InvalidKeywordReferenceRoot { keyword: _, context: _ }
            | Self::MissingDynamicDeclaration { context: _ }
            | Self::UnknownInputFieldReference { field_name: _, context: _ }
            | Self::UnknownDynamicFieldReference { field_name: _, context: _ }
            | Self::UnknownSecretsFieldReference { field_name: _, context: _ }
            | Self::SecretReferenceInLlmContext {
                reference_path: _,
                context: _,
            }
            | Self::MissingAgentOutputTypeForFieldReference { agent_name: _, context: _ }
            | Self::MissingOptionalReferenceAccess {
                reference_path: _,
                field_name: _,
                context: _,
            }
            | Self::InvalidReferencePath {
                reference_path: _,
                invalid_field: _,
                context: _,
            }
            | Self::InvalidForLoopIterableType {
                agent_name: _,
                found_type: _,
            }
            | Self::UnknownSchemaReference {
                referenced_schema: _,
                context: _,
            }
            | Self::UnknownToolReference {
                tool_name: _,
                agent_name: _,
            }
            | Self::InvalidTypeExpressionReference {
                reference_path: _,
                context: _,
            }
            | Self::AgentDependencyCycle { agent_names: _ }
            | Self::MissingInputDeclaration { context: _ }
            | Self::MissingSecretsDeclaration { context: _ } => Some(self.reference_resolution_help_message()),
        }
    }

    fn duplicate_declaration_help_message(&self) -> String {
        match self {
            Self::DuplicateProvider { provider_name } => {
                format!("Keep a single `provider {provider_name}` declaration, or rename one provider.")
            }
            Self::DuplicateSchema { schema_name } => {
                format!("Keep a single `schema {schema_name}` declaration, or rename one schema.")
            }
            Self::DuplicateTool { tool_name } => {
                format!("Keep a single `tool {tool_name}` declaration, or rename one tool.")
            }
            Self::DuplicateAgent { agent_name } => {
                format!("Keep a single `agent {agent_name}` declaration, or rename one agent.")
            }
            Self::DuplicateSingletonDeclaration { declaration_kind } => {
                format!(
                    "Only one `{}` declaration is allowed; merge fields into a single block.",
                    declaration_kind.as_str()
                )
            }
            Self::DuplicateProperty { property_name, context } => {
                format!(
                    "Keep a single `{property_name}` entry in {} and remove duplicate definitions.",
                    context.describe()
                )
            }
            _ => "Remove duplicate declarations to make names unique.".to_string(),
        }
    }

    fn agent_model_help_message(&self) -> String {
        match self {
            Self::InvalidInferenceSettingValueType {
                agent_name: _,
                inference_setting,
            } => {
                format!(
                    "Set `{}` to {}.",
                    inference_setting.key(),
                    inference_setting.expected_value_description()
                )
            }
            Self::InvalidModelExpression { agent_name: _ } => {
                "Use `model: provider_name(\"model-name\")` or `model: provider_name(expression)` with exactly one model argument."
                    .to_string()
            }
            Self::UnknownProviderInModel {
                agent_name: _,
                provider_name,
            } => {
                format!("Declare `provider {provider_name} {{ ... }}` or update `model` to use an existing provider.")
            }
            Self::UnknownModelForProvider {
                agent_name: _,
                provider_name,
                model_name,
            } => {
                format!("Add `{model_name}` to `provider {provider_name}` `models`, or choose a model already listed there.")
            }
            _ => "Fix agent model and inference settings to use valid values.".to_string(),
        }
    }

    fn reference_resolution_help_message(&self) -> String {
        match self {
            Self::UnknownAgentReference {
                referenced_agent,
                context: _,
            } => {
                format!("Declare `agent {referenced_agent} {{ ... }}` before this reference, or fix the agent name.")
            }
            Self::InvalidKeywordReferenceRoot { keyword, context: _ } => {
                format!("Add a field path after `{}`.", keyword.as_str())
            }
            Self::MissingDynamicDeclaration { context: _ } => {
                "Add a `dynamic { ... }` block with the fields used by `dynamic.<field>` references.".to_string()
            }
            Self::MissingInputDeclaration { context: _ } => {
                "Add an `input { ... }` declaration with the fields used by `input.<field>` references.".to_string()
            }
            Self::MissingSecretsDeclaration { context: _ } => {
                "Add a `secrets { ... }` declaration with the fields used by `secrets.<field>` references.".to_string()
            }
            Self::UnknownInputFieldReference { field_name, context: _ } => {
                format!("Add `{field_name}: <type>` to `input`, or reference an existing input field.")
            }
            Self::UnknownDynamicFieldReference { field_name, context: _ } => {
                format!("Add `{field_name}: <value>` to a `dynamic` block, or reference an existing dynamic field.")
            }
            Self::UnknownSecretsFieldReference { field_name, context: _ } => {
                format!("Add `{field_name}: <type>` to `secrets`, or reference an existing secrets field.")
            }
            Self::SecretReferenceInLlmContext {
                reference_path,
                context: _,
            } => {
                format!(
                    "Do not use `{reference_path}` in prompts/output; keep secrets in provider or tool configuration and pass safe derived values instead."
                )
            }
            Self::MissingAgentOutputTypeForFieldReference { agent_name, context: _ } => {
                format!("Add `output: <type>` to `agent {agent_name}` before referencing `agent.{agent_name}` or its fields.")
            }
            Self::MissingOptionalReferenceAccess {
                reference_path: _,
                field_name,
                context: _,
            } => {
                format!("Use `?.{field_name}` for this access, or refine the type so the preceding path cannot be `null`.")
            }
            Self::InvalidReferencePath {
                reference_path: _,
                invalid_field,
                context: _,
            } => {
                format!("Ensure the referenced type contains `{invalid_field}`, or update the reference path to an existing field.")
            }
            Self::InvalidForLoopIterableType {
                agent_name: _,
                found_type: _,
            } => "Use an array expression in the `in` clause, such as `agent.other_agent.items` or an array literal.".to_string(),
            Self::UnknownSchemaReference {
                referenced_schema,
                context: _,
            } => {
                format!("Declare `schema {referenced_schema} {{ ... }}` before using `schema.{referenced_schema}`.")
            }
            Self::UnknownToolReference { tool_name, agent_name: _ } => {
                format!("Declare `tool {tool_name} {{ ... }}` before using `tool.{tool_name}` in an agent `tools` list.")
            }
            Self::InvalidTypeExpressionReference {
                reference_path: _,
                context: _,
            } => {
                "Use a scalar type (`string`, `number`, `float`, `boolean`, `null`), `schema.<name>`, or a reference that starts with `agent.` or `input.`."
                    .to_string()
            }
            Self::AgentDependencyCycle { agent_names } => {
                format!(
                    "Break the cycle by removing at least one dependency among: {}.",
                    agent_names.join(" -> ")
                )
            }
            _ => "Fix reference paths and declarations so all references resolve.".to_string(),
        }
    }

    fn unknown_agent_property_help(property_name: &str) -> String {
        if let Some(suggested_property_name) = AgentPropertyName::suggested_from_identifier(property_name) {
            return format!(
                "Did you mean `{}`? Supported properties: {}.",
                suggested_property_name.as_str(),
                AgentPropertyName::rendered_values()
            );
        }

        format!("Supported properties: {}.", AgentPropertyName::rendered_values())
    }
}

impl From<&ValidationIssue> for DiagnosticCode {
    fn from(validation_issue: &ValidationIssue) -> Self {
        match validation_issue {
            ValidationIssue::DuplicateProvider { provider_name: _ } => Self::DuplicateProvider,
            ValidationIssue::DuplicateSchema { schema_name: _ } => Self::DuplicateSchema,
            ValidationIssue::DuplicateTool { tool_name: _ } => Self::DuplicateTool,
            ValidationIssue::DuplicateAgent { agent_name: _ } => Self::DuplicateAgent,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind: _ } => Self::DuplicateSingletonDeclaration,
            ValidationIssue::DuplicateProperty {
                property_name: _,
                context: _,
            } => Self::DuplicateProperty,
            ValidationIssue::UnknownAgentProperty {
                agent_name: _,
                property_name: _,
            } => Self::UnknownAgentProperty,
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name: _,
                inference_setting: _,
            } => Self::InvalidInferenceSettingValueType,
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
            ValidationIssue::MissingDynamicDeclaration { context: _ } => Self::MissingDynamicDeclaration,
            ValidationIssue::MissingInputDeclaration { context: _ } => Self::MissingInputDeclaration,
            ValidationIssue::MissingSecretsDeclaration { context: _ } => Self::MissingSecretsDeclaration,
            ValidationIssue::UnknownInputFieldReference { field_name: _, context: _ } => Self::UnknownInputFieldReference,
            ValidationIssue::UnknownDynamicFieldReference { field_name: _, context: _ } => Self::UnknownDynamicFieldReference,
            ValidationIssue::UnknownSecretsFieldReference { field_name: _, context: _ } => Self::UnknownSecretsFieldReference,
            ValidationIssue::SecretReferenceInLlmContext {
                reference_path: _,
                context: _,
            } => Self::SecretReferenceInLlmContext,
            ValidationIssue::MissingAgentOutputTypeForFieldReference { agent_name: _, context: _ } => {
                Self::MissingAgentOutputTypeForFieldReference
            }
            ValidationIssue::MissingOptionalReferenceAccess {
                reference_path: _,
                field_name: _,
                context: _,
            } => Self::MissingOptionalReferenceAccess,
            ValidationIssue::InvalidReferencePath {
                reference_path: _,
                invalid_field: _,
                context: _,
            } => Self::InvalidReferencePath,
            ValidationIssue::InvalidForLoopIterableType {
                agent_name: _,
                found_type: _,
            } => Self::InvalidForLoopIterableType,
            ValidationIssue::UnknownSchemaReference {
                referenced_schema: _,
                context: _,
            } => Self::UnknownSchemaReference,
            ValidationIssue::UnknownToolReference {
                tool_name: _,
                agent_name: _,
            } => Self::UnknownToolReference,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path: _,
                context: _,
            } => Self::InvalidTypeExpressionReference,
            ValidationIssue::AgentDependencyCycle { agent_names: _ } => Self::AgentDependencyCycle,
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
    tool_names: HashSet<String>,
    schema_names: HashSet<String>,
    schema_field_types: HashMap<String, HashMap<String, TypeExpression>>,
    input_field_types: Option<HashMap<String, TypeExpression>>,
    secrets_field_types: Option<HashMap<String, TypeExpression>>,
    agent_output_types: HashMap<String, Option<TypeExpression>>,
    tool_input_types: HashMap<String, crate::runtime::types::WorkflowType>,
    tool_binding_types: HashMap<String, crate::runtime::types::WorkflowType>,
    tool_output_types: HashMap<String, crate::runtime::types::WorkflowType>,
}

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut validation_report = ValidationReport::default();
    let validation_index = build_validation_index(workflow, &mut validation_report);

    validate_duplicate_properties(workflow, &mut validation_report);
    validate_schema_references(workflow, &validation_index, &mut validation_report);
    validate_agent_inference_settings(workflow, &mut validation_report);
    validate_agent_model_bindings(workflow, &validation_index, &mut validation_report);
    validate_agent_tool_references(workflow, &validation_index, &mut validation_report);
    validate_agent_references(workflow, &validation_index, &mut validation_report);
    validate_agent_dependency_cycles(workflow, &validation_index, &mut validation_report);

    validation_report
}

#[allow(clippy::too_many_lines)]
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
            Declaration::Tool(tool_declaration) => {
                let inserted_tool = validation_index.tool_names.insert(tool_declaration.name.clone());

                let named_schema_types = validation_index
                    .schema_field_types
                    .iter()
                    .map(|(schema_name, field_types)| {
                        (
                            schema_name.clone(),
                            TypeExpression::Object(
                                field_types
                                    .iter()
                                    .map(|(field_name, field_type)| TypedField {
                                        name: field_name.clone(),
                                        field_type: field_type.clone(),
                                        description: None,
                                        span: tool_declaration.span,
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                        )
                    })
                    .collect::<HashMap<_, _>>();

                if let Ok(tool_input_type) =
                    workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                {
                    validation_index
                        .tool_input_types
                        .insert(tool_declaration.name.clone(), tool_input_type);
                }

                if let Ok(tool_binding_type) = workflow_type_from_dsl(
                    &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                    &named_schema_types,
                ) {
                    validation_index
                        .tool_binding_types
                        .insert(tool_declaration.name.clone(), tool_binding_type);
                }

                if let Ok(tool_output_type) =
                    workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                {
                    validation_index
                        .tool_output_types
                        .insert(tool_declaration.name.clone(), tool_output_type);
                }

                if !inserted_tool {
                    validation_report.push_issue_with_span(
                        ValidationIssue::DuplicateTool {
                            tool_name: tool_declaration.name.clone(),
                        },
                        Some(tool_declaration.span),
                    );
                }
            }
            Declaration::Dynamic(_) => {}
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

                let agent_output_type = agent_declaration.output_type().cloned();
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

#[allow(clippy::too_many_lines)]
fn validate_duplicate_properties(workflow: &Workflow, validation_report: &mut ValidationReport) {
    let mut seen_workflow_dynamic_field_names = HashSet::<String>::new();

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                report_duplicate_object_field_names(
                    provider_declaration.properties.as_slice(),
                    provider_context.clone(),
                    Some(provider_declaration.span),
                    validation_report,
                );

                for provider_property in &provider_declaration.properties {
                    report_duplicate_expression_object_fields(
                        &provider_property.value,
                        provider_context.clone(),
                        Some(provider_declaration.span),
                        validation_report,
                    );
                }
            }
            Declaration::Schema(schema_declaration) => {
                let schema_context = ValidationContext::Schema(schema_declaration.name.clone());

                report_duplicate_typed_field_names(schema_declaration.fields.as_slice(), schema_context.clone(), validation_report);

                for schema_field in &schema_declaration.fields {
                    report_duplicate_type_expression_fields(&schema_field.field_type, schema_context.clone(), validation_report);
                }
            }
            Declaration::Tool(tool_declaration) => {
                let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                report_duplicate_typed_field_names(tool_declaration.input_fields.as_slice(), tool_context.clone(), validation_report);
                report_duplicate_typed_field_names(tool_declaration.binding_fields.as_slice(), tool_context.clone(), validation_report);
                report_duplicate_typed_field_names(tool_declaration.output_fields.as_slice(), tool_context.clone(), validation_report);

                for input_field in &tool_declaration.input_fields {
                    report_duplicate_type_expression_fields(&input_field.field_type, tool_context.clone(), validation_report);
                }

                for binding_field in &tool_declaration.binding_fields {
                    report_duplicate_type_expression_fields(&binding_field.field_type, tool_context.clone(), validation_report);
                }

                for output_field in &tool_declaration.output_fields {
                    report_duplicate_type_expression_fields(&output_field.field_type, tool_context.clone(), validation_report);
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                let mut seen_agent_properties = HashSet::<AgentPropertyName>::new();
                let mut seen_agent_dynamic_field_names = HashSet::<String>::new();

                for agent_property in &agent_declaration.properties {
                    let agent_property_name = agent_property.name();

                    if agent_property_name != AgentPropertyName::Dynamic && !seen_agent_properties.insert(agent_property_name) {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateProperty {
                                property_name: agent_property_name.as_str().to_string(),
                                context: agent_context.clone(),
                            },
                            Some(agent_declaration.span),
                        );
                    }

                    match agent_property {
                        AgentProperty::Dynamic(dynamic_block) => {
                            report_duplicate_object_field_names(
                                dynamic_block.fields.as_slice(),
                                agent_context.clone(),
                                Some(dynamic_block.span),
                                validation_report,
                            );

                            for dynamic_field in &dynamic_block.fields {
                                if !seen_agent_dynamic_field_names.insert(dynamic_field.name.clone()) {
                                    validation_report.push_issue_with_span(
                                        ValidationIssue::DuplicateProperty {
                                            property_name: dynamic_field.name.clone(),
                                            context: agent_context.clone(),
                                        },
                                        Some(dynamic_block.span),
                                    );
                                }

                                report_duplicate_expression_object_fields(
                                    &dynamic_field.value,
                                    agent_context.clone(),
                                    Some(dynamic_block.span),
                                    validation_report,
                                );
                            }
                        }
                        AgentProperty::Model(expression)
                        | AgentProperty::Prompt(expression)
                        | AgentProperty::Context(expression)
                        | AgentProperty::Inference(expression)
                        | AgentProperty::Tools(expression) => {
                            report_duplicate_expression_object_fields(
                                expression,
                                agent_context.clone(),
                                Some(agent_declaration.span),
                                validation_report,
                            );
                        }
                        AgentProperty::Output {
                            output_type_expression,
                            description: _,
                        } => {
                            report_duplicate_type_expression_fields(output_type_expression, agent_context.clone(), validation_report);
                        }
                    }
                }
            }
            Declaration::Input(input_declaration) => {
                let input_context = ValidationContext::Input;

                report_duplicate_typed_field_names(input_declaration.fields.as_slice(), input_context.clone(), validation_report);

                for input_field in &input_declaration.fields {
                    report_duplicate_type_expression_fields(&input_field.field_type, input_context.clone(), validation_report);
                }
            }
            Declaration::Secrets(secrets_declaration) => {
                let secrets_context = ValidationContext::Secrets;

                report_duplicate_typed_field_names(secrets_declaration.fields.as_slice(), secrets_context.clone(), validation_report);

                for secrets_field in &secrets_declaration.fields {
                    report_duplicate_type_expression_fields(&secrets_field.field_type, secrets_context.clone(), validation_report);
                }
            }
            Declaration::Output(output_declaration) => {
                let output_context = ValidationContext::Output;

                report_duplicate_object_field_names(
                    output_declaration.fields.as_slice(),
                    output_context.clone(),
                    Some(output_declaration.span),
                    validation_report,
                );

                for output_field in &output_declaration.fields {
                    report_duplicate_expression_object_fields(
                        &output_field.value,
                        output_context.clone(),
                        Some(output_declaration.span),
                        validation_report,
                    );
                }
            }
            Declaration::Dynamic(dynamic_block) => {
                report_duplicate_object_field_names(
                    dynamic_block.fields.as_slice(),
                    ValidationContext::Dynamic,
                    Some(dynamic_block.span),
                    validation_report,
                );

                for dynamic_field in &dynamic_block.fields {
                    if !seen_workflow_dynamic_field_names.insert(dynamic_field.name.clone()) {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateProperty {
                                property_name: dynamic_field.name.clone(),
                                context: ValidationContext::Dynamic,
                            },
                            Some(dynamic_block.span),
                        );
                    }

                    report_duplicate_expression_object_fields(
                        &dynamic_field.value,
                        ValidationContext::Dynamic,
                        Some(dynamic_block.span),
                        validation_report,
                    );
                }
            }
        }
    }
}

fn report_duplicate_object_field_names(
    object_fields: &[ObjectField],
    context: ValidationContext,
    duplicate_span: Option<SourceSpan>,
    validation_report: &mut ValidationReport,
) {
    let mut seen_field_names = HashSet::<String>::new();

    for object_field in object_fields {
        if seen_field_names.insert(object_field.name.clone()) {
            continue;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::DuplicateProperty {
                property_name: object_field.name.clone(),
                context: context.clone(),
            },
            duplicate_span,
        );
    }
}

fn report_duplicate_typed_field_names(typed_fields: &[TypedField], context: ValidationContext, validation_report: &mut ValidationReport) {
    let mut seen_field_names = HashSet::<String>::new();

    for typed_field in typed_fields {
        if seen_field_names.insert(typed_field.name.clone()) {
            continue;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::DuplicateProperty {
                property_name: typed_field.name.clone(),
                context: context.clone(),
            },
            Some(typed_field.span),
        );
    }
}

fn report_duplicate_type_expression_fields(
    type_expression: &TypeExpression,
    context: ValidationContext,
    validation_report: &mut ValidationReport,
) {
    match type_expression {
        TypeExpression::Array {
            item_type,
            fixed_length: _,
        } => {
            report_duplicate_type_expression_fields(item_type, context, validation_report);
        }
        TypeExpression::Tuple(tuple_items) | TypeExpression::Union(tuple_items) => {
            for tuple_item in tuple_items {
                report_duplicate_type_expression_fields(tuple_item, context.clone(), validation_report);
            }
        }
        TypeExpression::Object(typed_fields) => {
            report_duplicate_typed_field_names(typed_fields.as_slice(), context.clone(), validation_report);

            for typed_field in typed_fields {
                report_duplicate_type_expression_fields(&typed_field.field_type, context.clone(), validation_report);
            }
        }
        TypeExpression::String
        | TypeExpression::Number
        | TypeExpression::Float
        | TypeExpression::Boolean
        | TypeExpression::Null
        | TypeExpression::SchemaReference(_)
        | TypeExpression::StringEnum(_)
        | TypeExpression::StringEnumReference(_) => {}
    }
}

fn report_duplicate_expression_object_fields(
    expression: &Expression,
    context: ValidationContext,
    duplicate_span: Option<SourceSpan>,
    validation_report: &mut ValidationReport,
) {
    match expression {
        Expression::FunctionCall(function_call) => {
            for call_argument in &function_call.arguments {
                report_duplicate_expression_object_fields(call_argument.expression(), context.clone(), duplicate_span, validation_report);
            }
        }
        Expression::ToolCall(tool_call) => {
            report_duplicate_object_field_names(
                tool_call.input_fields.as_slice(),
                context.clone(),
                duplicate_span,
                validation_report,
            );
            report_duplicate_object_field_names(
                tool_call.binding_fields.as_slice(),
                context.clone(),
                duplicate_span,
                validation_report,
            );

            for object_field in &tool_call.input_fields {
                report_duplicate_expression_object_fields(&object_field.value, context.clone(), duplicate_span, validation_report);
            }

            for object_field in &tool_call.binding_fields {
                report_duplicate_expression_object_fields(&object_field.value, context.clone(), duplicate_span, validation_report);
            }
        }
        Expression::ArrayLiteral(array_values) => {
            for array_value in array_values {
                report_duplicate_expression_object_fields(array_value, context.clone(), duplicate_span, validation_report);
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            report_duplicate_object_field_names(object_fields.as_slice(), context.clone(), duplicate_span, validation_report);

            for object_field in object_fields {
                report_duplicate_expression_object_fields(&object_field.value, context.clone(), duplicate_span, validation_report);
            }
        }
        Expression::StringTemplate(string_template) => {
            for string_template_part in &string_template.parts {
                let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part else {
                    continue;
                };

                report_duplicate_expression_object_fields(interpolation_expression, context.clone(), duplicate_span, validation_report);
            }
        }
        Expression::StringLiteral(_)
        | Expression::NumberLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral
        | Expression::Reference(_) => {}
    }
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

fn validate_agent_inference_settings(workflow: &Workflow, validation_report: &mut ValidationReport) {
    let mut invalid_inference_setting_values = HashSet::<(String, InferenceSetting)>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        for agent_property in &agent_declaration.properties {
            let AgentProperty::Inference(inference_expression) = agent_property else {
                continue;
            };

            let Expression::ObjectLiteral(inference_fields) = inference_expression else {
                continue;
            };

            for inference_field in inference_fields {
                let Some(inference_setting) = InferenceSetting::from_identifier(inference_field.name.as_str()) else {
                    continue;
                };

                if inference_setting.accepts_expression(&inference_field.value) {
                    continue;
                }

                let issue_key = (agent_declaration.name.clone(), inference_setting);

                if !invalid_inference_setting_values.insert(issue_key.clone()) {
                    continue;
                }

                validation_report.push_issue_with_span(
                    ValidationIssue::InvalidInferenceSettingValueType {
                        agent_name: issue_key.0,
                        inference_setting: issue_key.1,
                    },
                    Some(agent_declaration.span),
                );
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn validate_schema_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut unknown_schema_references = HashSet::new();
    let mut invalid_type_expression_references = HashSet::new();

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
                        &mut invalid_type_expression_references,
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
                        &mut invalid_type_expression_references,
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
                        &mut invalid_type_expression_references,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());

                for agent_property in &agent_declaration.properties {
                    if let AgentProperty::Output {
                        output_type_expression,
                        description: _,
                    } = agent_property
                    {
                        validate_type_expression_for_schemas(
                            output_type_expression,
                            agent_context.clone(),
                            Some(agent_declaration.span),
                            validation_index,
                            validation_report,
                            &mut unknown_schema_references,
                            &mut invalid_type_expression_references,
                        );
                    }
                }
            }
            Declaration::Tool(tool_declaration) => {
                let tool_context = ValidationContext::Tool(tool_declaration.name.clone());

                for input_field in &tool_declaration.input_fields {
                    validate_type_expression_for_schemas(
                        &input_field.field_type,
                        tool_context.clone(),
                        Some(input_field.span),
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                        &mut invalid_type_expression_references,
                    );
                }

                for bounded_field in &tool_declaration.binding_fields {
                    validate_type_expression_for_schemas(
                        &bounded_field.field_type,
                        tool_context.clone(),
                        Some(bounded_field.span),
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                        &mut invalid_type_expression_references,
                    );
                }

                for output_field in &tool_declaration.output_fields {
                    validate_type_expression_for_schemas(
                        &output_field.field_type,
                        tool_context.clone(),
                        Some(output_field.span),
                        validation_index,
                        validation_report,
                        &mut unknown_schema_references,
                        &mut invalid_type_expression_references,
                    );
                }
            }
            Declaration::Provider(_) | Declaration::Dynamic(_) | Declaration::Output(_) => {}
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
    invalid_type_expression_references: &mut HashSet<(ValidationContext, String)>,
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
                invalid_type_expression_references,
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
                    invalid_type_expression_references,
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
                    invalid_type_expression_references,
                );
            }
        }
        TypeExpression::StringEnumReference(reference) => {
            let Some(reference_root_keyword) = reference.root_keyword() else {
                let reference_path = reference.render_path();
                let issue_key = (context.clone(), reference_path.clone());

                if invalid_type_expression_references.insert(issue_key) {
                    validation_report.push_issue_with_span(
                        ValidationIssue::InvalidTypeExpressionReference { reference_path, context },
                        Some(reference.span),
                    );
                }

                return;
            };

            if !matches!(reference_root_keyword, ReferenceKeyword::Agent | ReferenceKeyword::Input) {
                let reference_path = reference.render_path();
                let issue_key = (context.clone(), reference_path.clone());

                if invalid_type_expression_references.insert(issue_key) {
                    validation_report.push_issue_with_span(
                        ValidationIssue::InvalidTypeExpressionReference { reference_path, context },
                        Some(reference.span),
                    );
                }
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

    let model_argument_expressions = model_call.model_argument_expressions();

    if model_argument_expressions.is_empty() {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            model_span,
        );

        return;
    }

    if model_argument_expressions.len() > 1 {
        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelExpression {
                agent_name: agent_name.to_owned(),
            },
            model_span,
        );

        return;
    }

    let Expression::StringLiteral(model_name) = model_argument_expressions[0] else {
        return;
    };

    let Some(declared_models) = &provider_info.declared_models else {
        return;
    };

    if declared_models.contains(model_name) {
        return;
    }

    validation_report.push_issue_with_span(
        ValidationIssue::UnknownModelForProvider {
            agent_name: agent_name.to_owned(),
            provider_name,
            model_name: model_name.clone(),
        },
        model_span,
    );
}

fn validate_agent_tool_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut reported_unknown_tools = HashSet::<(String, String)>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let Some(tools_expression) = agent_declaration.expression_property(crate::dsl::AgentExpressionPropertyName::Tools) else {
            continue;
        };

        for tool_name in tools_expression.referenced_tool_names() {
            if validation_index.tool_names.contains(&tool_name) {
                continue;
            }

            let issue_key = (agent_declaration.name.clone(), tool_name.clone());

            if !reported_unknown_tools.insert(issue_key.clone()) {
                continue;
            }

            validation_report.push_issue_with_span(
                ValidationIssue::UnknownToolReference {
                    agent_name: issue_key.0,
                    tool_name: issue_key.1,
                },
                Some(agent_declaration.span),
            );
        }
    }
}

trait ToolReferenceCollector {
    fn referenced_tool_names(&self) -> Vec<String>;
}

impl ToolReferenceCollector for Expression {
    fn referenced_tool_names(&self) -> Vec<String> {
        let Expression::ArrayLiteral(tool_expressions) = self else {
            return Vec::new();
        };

        tool_expressions.iter().filter_map(Expression::direct_tool_name).collect()
    }
}

trait DirectToolName {
    fn direct_tool_name(&self) -> Option<String>;
}

impl DirectToolName for Expression {
    fn direct_tool_name(&self) -> Option<String> {
        match self {
            Self::Reference(reference) => reference.direct_tool_name(),
            Self::FunctionCall(function_call) => function_call.direct_tool_name(),
            Self::ToolCall(tool_call) => tool_call.callee.direct_tool_name(),
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }
}

impl DirectToolName for FunctionCall {
    fn direct_tool_name(&self) -> Option<String> {
        self.callee.direct_tool_name()
    }
}

impl DirectToolName for Reference {
    fn direct_tool_name(&self) -> Option<String> {
        if self.root_keyword() != Some(ReferenceKeyword::Tool) || self.accesses.len() != 1 || self.accesses[0].optional {
            return None;
        }

        Some(self.accesses[0].field.clone())
    }
}

#[allow(clippy::too_many_lines)]
fn validate_agent_references(workflow: &Workflow, validation_index: &ValidationIndex, validation_report: &mut ValidationReport) {
    let mut keyword_reference_validation_state = KeywordReferenceValidationState::new(workflow, validation_index, validation_report);
    let mut workflow_dynamic_field_types = keyword_reference_validation_state
        .for_loop_type_inference_context
        .local_binding_types
        .clone();

    for declaration in workflow.declarations() {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                let provider_context = ValidationContext::Provider(provider_declaration.name.clone());

                for provider_property in &provider_declaration.properties {
                    keyword_reference_validation_state.validate_expression(
                        &provider_property.value,
                        &workflow_dynamic_field_types,
                        provider_context.clone(),
                        SecretReferencePolicy::Allow,
                    );
                }
            }
            Declaration::Dynamic(dynamic_block) => {
                for dynamic_field in &dynamic_block.fields {
                    keyword_reference_validation_state.validate_expression(
                        &dynamic_field.value,
                        &workflow_dynamic_field_types,
                        ValidationContext::Dynamic,
                        SecretReferencePolicy::Allow,
                    );

                    keyword_reference_validation_state.infer_dynamic_field_type(
                        &dynamic_field.name,
                        &dynamic_field.value,
                        &mut workflow_dynamic_field_types,
                    );
                }
            }
            Declaration::Agent(agent_declaration) => {
                let agent_context = ValidationContext::Agent(agent_declaration.name.clone());
                let mut agent_dynamic_field_types = workflow_dynamic_field_types.clone();

                if let Some(agent_for_loop) = &agent_declaration.for_loop {
                    keyword_reference_validation_state.validate_expression(
                        &agent_for_loop.iterable,
                        &agent_dynamic_field_types,
                        agent_context.clone(),
                        SecretReferencePolicy::Allow,
                    );

                    keyword_reference_validation_state.validate_for_loop_iterable_type(agent_declaration, agent_for_loop);

                    if let Some(iterable_item_type) = keyword_reference_validation_state.infer_for_loop_item_type(agent_for_loop) {
                        for bound_identifier_name in agent_for_loop.bound_identifier_names() {
                            agent_dynamic_field_types.insert(bound_identifier_name.to_string(), iterable_item_type.clone());
                        }
                    }
                }

                for agent_property in &agent_declaration.properties {
                    match agent_property {
                        AgentProperty::Prompt(model_expression) | AgentProperty::Context(model_expression) => {
                            keyword_reference_validation_state.validate_expression(
                                model_expression,
                                &agent_dynamic_field_types,
                                agent_context.clone(),
                                SecretReferencePolicy::Forbid,
                            );
                        }
                        AgentProperty::Dynamic(dynamic_block) => {
                            for dynamic_field in &dynamic_block.fields {
                                keyword_reference_validation_state.validate_expression(
                                    &dynamic_field.value,
                                    &agent_dynamic_field_types,
                                    agent_context.clone(),
                                    SecretReferencePolicy::Allow,
                                );

                                keyword_reference_validation_state.infer_dynamic_field_type(
                                    &dynamic_field.name,
                                    &dynamic_field.value,
                                    &mut agent_dynamic_field_types,
                                );
                            }
                        }
                        AgentProperty::Model(model_expression)
                        | AgentProperty::Inference(model_expression)
                        | AgentProperty::Tools(model_expression) => {
                            keyword_reference_validation_state.validate_expression(
                                model_expression,
                                &agent_dynamic_field_types,
                                agent_context.clone(),
                                SecretReferencePolicy::Allow,
                            );
                        }
                        AgentProperty::Output {
                            output_type_expression: _,
                            description: _,
                        } => {}
                    }
                }
            }
            Declaration::Output(output_declaration) => {
                for output_field in &output_declaration.fields {
                    keyword_reference_validation_state.validate_expression(
                        &output_field.value,
                        &workflow_dynamic_field_types,
                        ValidationContext::Output,
                        SecretReferencePolicy::Forbid,
                    );
                }
            }
            Declaration::Secrets(_) | Declaration::Input(_) | Declaration::Schema(_) | Declaration::Tool(_) => {}
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
    for_loop_type_inference_context: TypeInferenceContext,
    unknown_agent_references: HashSet<(ValidationContext, String)>,
    invalid_keyword_reference_roots: HashSet<(ValidationContext, ReferenceKeyword)>,
    secret_reference_leaks: HashSet<(ValidationContext, String)>,
    missing_agent_output_type_references: HashSet<(ValidationContext, String)>,
    missing_optional_reference_accesses: HashSet<(ValidationContext, String, String)>,
    invalid_reference_paths: HashSet<(ValidationContext, String, String)>,
    missing_dynamic_declaration_contexts: HashSet<ValidationContext>,
    missing_input_declaration_contexts: HashSet<ValidationContext>,
    missing_secrets_declaration_contexts: HashSet<ValidationContext>,
    unknown_dynamic_field_references: HashSet<(ValidationContext, String)>,
    unknown_input_field_references: HashSet<(ValidationContext, String)>,
    unknown_secrets_field_references: HashSet<(ValidationContext, String)>,
}

impl<'validation> KeywordReferenceValidationState<'validation> {
    fn new(
        workflow: &Workflow,
        validation_index: &'validation ValidationIndex,
        validation_report: &'validation mut ValidationReport,
    ) -> Self {
        let for_loop_type_inference_context = Self::build_for_loop_type_inference_context(workflow);

        Self {
            validation_index,
            validation_report,
            for_loop_type_inference_context,
            unknown_agent_references: HashSet::new(),
            invalid_keyword_reference_roots: HashSet::new(),
            secret_reference_leaks: HashSet::new(),
            missing_agent_output_type_references: HashSet::new(),
            missing_optional_reference_accesses: HashSet::new(),
            invalid_reference_paths: HashSet::new(),
            missing_dynamic_declaration_contexts: HashSet::new(),
            missing_input_declaration_contexts: HashSet::new(),
            missing_secrets_declaration_contexts: HashSet::new(),
            unknown_dynamic_field_references: HashSet::new(),
            unknown_input_field_references: HashSet::new(),
            unknown_secrets_field_references: HashSet::new(),
        }
    }

    fn build_for_loop_type_inference_context(workflow: &Workflow) -> TypeInferenceContext {
        let mut named_schema_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Schema(schema_declaration) = declaration else {
                continue;
            };

            named_schema_types.insert(
                schema_declaration.name.clone(),
                TypeExpression::Object(schema_declaration.fields.clone()),
            );
        }

        let input_type = workflow.find_input().and_then(|input_declaration| {
            workflow_type_from_dsl(&TypeExpression::Object(input_declaration.fields.clone()), &named_schema_types).ok()
        });

        let secrets_type = workflow.find_secrets().and_then(|secrets_declaration| {
            workflow_type_from_dsl(&TypeExpression::Object(secrets_declaration.fields.clone()), &named_schema_types).ok()
        });

        let mut agent_output_types = HashMap::new();
        let mut tool_input_types = HashMap::new();
        let mut tool_binding_types = HashMap::new();
        let mut tool_output_types = HashMap::new();

        for declaration in workflow.declarations() {
            if let Declaration::Tool(tool_declaration) = declaration {
                if let Ok(tool_input_type) =
                    workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                {
                    tool_input_types.insert(tool_declaration.name.clone(), tool_input_type);
                }

                if let Ok(tool_binding_type) = workflow_type_from_dsl(
                    &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                    &named_schema_types,
                ) {
                    tool_binding_types.insert(tool_declaration.name.clone(), tool_binding_type);
                }

                if let Ok(tool_output_type) =
                    workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                {
                    tool_output_types.insert(tool_declaration.name.clone(), tool_output_type);
                }

                continue;
            }

            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let final_output_type_expression = agent_declaration.inferred_final_output_type_expression();
            let inferred_output_type = workflow_type_from_dsl(&final_output_type_expression, &named_schema_types);

            let Ok(inferred_output_type) = inferred_output_type else {
                continue;
            };

            agent_output_types.insert(agent_declaration.name.clone(), inferred_output_type);
        }

        let mut local_binding_types = HashMap::new();

        let mut type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            tool_input_types,
            tool_binding_types,
            tool_output_types,
            local_binding_types: HashMap::new(),
        };

        for dynamic_block in workflow.dynamic_blocks() {
            for dynamic_field in &dynamic_block.fields {
                let Ok(dynamic_field_type) = infer_expression_type(
                    &dynamic_field.value,
                    &type_inference_context,
                    &format!("dynamic field `{}` type inference", dynamic_field.name),
                ) else {
                    continue;
                };

                local_binding_types.insert(dynamic_field.name.clone(), dynamic_field_type.clone());
                type_inference_context
                    .local_binding_types
                    .insert(dynamic_field.name.clone(), dynamic_field_type);
            }
        }

        type_inference_context
    }

    fn validate_for_loop_iterable_type(&mut self, agent_declaration: &AgentDeclaration, agent_for_loop: &AgentForLoop) {
        let type_inference_context = &self.for_loop_type_inference_context;
        let inferred_iterable_type = infer_expression_type(
            &agent_for_loop.iterable,
            type_inference_context,
            &format!("for-loop iterable for agent `{}`", agent_declaration.name),
        );

        let Ok(inferred_iterable_type) = inferred_iterable_type else {
            return;
        };

        if inferred_iterable_type.is_guaranteed_array() {
            return;
        }

        self.validation_report.push_issue_with_span(
            ValidationIssue::InvalidForLoopIterableType {
                agent_name: agent_declaration.name.clone(),
                found_type: inferred_iterable_type.to_string(),
            },
            Some(agent_declaration.span),
        );
    }

    fn infer_for_loop_item_type(&self, agent_for_loop: &AgentForLoop) -> Option<crate::runtime::types::WorkflowType> {
        let inferred_iterable_type = infer_expression_type(
            &agent_for_loop.iterable,
            &self.for_loop_type_inference_context,
            "for-loop iterable item inference",
        )
        .ok()?;

        match inferred_iterable_type {
            crate::runtime::types::WorkflowType::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            crate::runtime::types::WorkflowType::Union(union_members) => {
                union_members.into_iter().find_map(|union_member| match union_member {
                    crate::runtime::types::WorkflowType::Array {
                        item_type,
                        fixed_length: _,
                    } => Some(*item_type),
                    crate::runtime::types::WorkflowType::String
                    | crate::runtime::types::WorkflowType::Integer
                    | crate::runtime::types::WorkflowType::Float
                    | crate::runtime::types::WorkflowType::Boolean
                    | crate::runtime::types::WorkflowType::Null
                    | crate::runtime::types::WorkflowType::StringEnum(_)
                    | crate::runtime::types::WorkflowType::Union(_)
                    | crate::runtime::types::WorkflowType::Tuple(_)
                    | crate::runtime::types::WorkflowType::Object(_) => None,
                })
            }
            crate::runtime::types::WorkflowType::String
            | crate::runtime::types::WorkflowType::Integer
            | crate::runtime::types::WorkflowType::Float
            | crate::runtime::types::WorkflowType::Boolean
            | crate::runtime::types::WorkflowType::Null
            | crate::runtime::types::WorkflowType::StringEnum(_)
            | crate::runtime::types::WorkflowType::Tuple(_)
            | crate::runtime::types::WorkflowType::Object(_) => None,
        }
    }

    fn infer_dynamic_field_type(
        &self,
        field_name: &str,
        expression: &Expression,
        dynamic_field_types: &mut HashMap<String, crate::runtime::types::WorkflowType>,
    ) {
        let mut type_inference_context = self.for_loop_type_inference_context.clone();
        type_inference_context.local_binding_types.clone_from(dynamic_field_types);

        let Ok(dynamic_field_type) = infer_expression_type(expression, &type_inference_context, field_name) else {
            return;
        };

        dynamic_field_types.insert(field_name.to_string(), dynamic_field_type);
    }

    fn validate_expression(
        &mut self,
        expression: &Expression,
        dynamic_field_types: &HashMap<String, crate::runtime::types::WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
        match expression {
            Expression::Reference(reference) => {
                self.validate_reference(reference, dynamic_field_types, context, secret_reference_policy);
            }
            Expression::FunctionCall(function_call) => {
                self.validate_reference(&function_call.callee, dynamic_field_types, context.clone(), secret_reference_policy);

                for call_argument in &function_call.arguments {
                    self.validate_expression(
                        call_argument.expression(),
                        dynamic_field_types,
                        context.clone(),
                        secret_reference_policy,
                    );
                }
            }
            Expression::ToolCall(tool_call) => {
                self.validate_reference(&tool_call.callee, dynamic_field_types, context.clone(), secret_reference_policy);

                for object_field in &tool_call.input_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }

                for object_field in &tool_call.binding_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    self.validate_expression(array_value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    self.validate_expression(&object_field.value, dynamic_field_types, context.clone(), secret_reference_policy);
                }
            }
            Expression::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        self.validate_expression(
                            interpolation_expression,
                            dynamic_field_types,
                            context.clone(),
                            secret_reference_policy,
                        );
                    }
                }
            }
            Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
        }
    }

    fn validate_reference(
        &mut self,
        reference: &Reference,
        dynamic_field_types: &HashMap<String, crate::runtime::types::WorkflowType>,
        context: ValidationContext,
        secret_reference_policy: SecretReferencePolicy,
    ) {
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
            ReferenceKeyword::Dynamic => {
                self.validate_dynamic_reference(reference, dynamic_field_types, context);
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

        let referenced_agent_output_type = self
            .validation_index
            .agent_output_types
            .get(referenced_agent_name)
            .and_then(Clone::clone);

        if reference.accesses.len() == 1 {
            if context == ValidationContext::Output && referenced_agent_output_type.is_none() {
                self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference.span);
            }

            return;
        }

        let Some(agent_output_type) = referenced_agent_output_type else {
            self.push_missing_agent_output_type_reference_issue(referenced_agent_name, context, reference.span);

            return;
        };

        self.validate_reference_path(reference, 1, agent_output_type, context);
    }

    fn push_missing_agent_output_type_reference_issue(
        &mut self,
        referenced_agent_name: &str,
        context: ValidationContext,
        reference_span: SourceSpan,
    ) {
        let issue_key = (context.clone(), referenced_agent_name.to_owned());

        if self.missing_agent_output_type_references.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::MissingAgentOutputTypeForFieldReference {
                    agent_name: referenced_agent_name.to_owned(),
                    context,
                },
                Some(reference_span),
            );
        }
    }

    fn validate_dynamic_reference(
        &mut self,
        reference: &Reference,
        dynamic_field_types: &HashMap<String, crate::runtime::types::WorkflowType>,
        context: ValidationContext,
    ) {
        let referenced_field_name = reference
            .accesses
            .first()
            .expect("dynamic reference should include first field after root")
            .field
            .as_str();

        let Some(dynamic_field_type) = dynamic_field_types.get(referenced_field_name) else {
            if dynamic_field_types.is_empty() {
                if self.missing_dynamic_declaration_contexts.insert(context.clone()) {
                    self.validation_report
                        .push_issue_with_span(ValidationIssue::MissingDynamicDeclaration { context }, Some(reference.span));
                }

                return;
            }

            let issue_key = (context.clone(), referenced_field_name.to_owned());

            if self.unknown_dynamic_field_references.insert(issue_key) {
                self.validation_report.push_issue_with_span(
                    ValidationIssue::UnknownDynamicFieldReference {
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

        self.validate_workflow_type_reference_path(reference, 1, dynamic_field_type.clone(), context);
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
            if candidate_types.iter().any(TypeExpression::can_be_null) && !reference_access.optional {
                self.push_missing_optional_reference_access(reference, reference_access.field.as_str(), context.clone());

                return;
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                self.collect_next_types_for_field(candidate_type, reference_access.field.as_str(), &mut next_candidate_types);
            }

            if reference_access.optional {
                next_candidate_types.push(TypeExpression::Null);
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

    fn validate_workflow_type_reference_path(
        &mut self,
        reference: &Reference,
        path_start_index: usize,
        start_type: crate::runtime::types::WorkflowType,
        context: ValidationContext,
    ) {
        let mut candidate_types = vec![start_type];

        for reference_access in reference.accesses.iter().skip(path_start_index) {
            if candidate_types.iter().any(workflow_type_can_be_null) && !reference_access.optional {
                self.push_missing_optional_reference_access(reference, reference_access.field.as_str(), context.clone());

                return;
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                Self::collect_next_workflow_types_for_field(candidate_type, reference_access.field.as_str(), &mut next_candidate_types);
            }

            if reference_access.optional {
                next_candidate_types.push(crate::runtime::types::WorkflowType::Null);
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

    fn push_missing_optional_reference_access(&mut self, reference: &Reference, field_name: &str, context: ValidationContext) {
        let reference_path = self.reference_to_string(reference);
        let issue_key = (context.clone(), reference_path.clone(), field_name.to_owned());

        if self.missing_optional_reference_accesses.insert(issue_key) {
            self.validation_report.push_issue_with_span(
                ValidationIssue::MissingOptionalReferenceAccess {
                    reference_path,
                    field_name: field_name.to_owned(),
                    context,
                },
                Some(reference.span),
            );
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
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => {}
        }
    }

    fn collect_next_workflow_types_for_field(
        candidate_type: &crate::runtime::types::WorkflowType,
        field_name: &str,
        next_candidate_types: &mut Vec<crate::runtime::types::WorkflowType>,
    ) {
        match candidate_type {
            crate::runtime::types::WorkflowType::Object(fields) => {
                if let Some(field_type) = fields.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            crate::runtime::types::WorkflowType::Union(union_members) => {
                for union_member in union_members {
                    Self::collect_next_workflow_types_for_field(union_member, field_name, next_candidate_types);
                }
            }
            crate::runtime::types::WorkflowType::String
            | crate::runtime::types::WorkflowType::Integer
            | crate::runtime::types::WorkflowType::Float
            | crate::runtime::types::WorkflowType::Boolean
            | crate::runtime::types::WorkflowType::Null
            | crate::runtime::types::WorkflowType::StringEnum(_)
            | crate::runtime::types::WorkflowType::Array {
                item_type: _,
                fixed_length: _,
            }
            | crate::runtime::types::WorkflowType::Tuple(_) => {}
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

fn workflow_type_can_be_null(workflow_type: &crate::runtime::types::WorkflowType) -> bool {
    match workflow_type {
        crate::runtime::types::WorkflowType::Null => true,
        crate::runtime::types::WorkflowType::Union(union_members) => union_members.iter().any(workflow_type_can_be_null),
        crate::runtime::types::WorkflowType::String
        | crate::runtime::types::WorkflowType::Integer
        | crate::runtime::types::WorkflowType::Float
        | crate::runtime::types::WorkflowType::Boolean
        | crate::runtime::types::WorkflowType::StringEnum(_)
        | crate::runtime::types::WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | crate::runtime::types::WorkflowType::Tuple(_)
        | crate::runtime::types::WorkflowType::Object(_) => false,
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
                AgentProperty::Dynamic(dynamic_block) => {
                    for dynamic_field in &dynamic_block.fields {
                        collect_agent_dependencies_from_expression(&dynamic_field.value, &mut referenced_agents);
                    }
                }
                AgentProperty::Model(model_expression)
                | AgentProperty::Prompt(model_expression)
                | AgentProperty::Context(model_expression)
                | AgentProperty::Inference(model_expression)
                | AgentProperty::Tools(model_expression) => {
                    collect_agent_dependencies_from_expression(model_expression, &mut referenced_agents);
                }
                AgentProperty::Output {
                    output_type_expression: _,
                    description: _,
                } => {}
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
        Expression::ToolCall(tool_call) => {
            collect_agent_dependency_from_reference(&tool_call.callee, referenced_agents);

            for object_field in &tool_call.input_fields {
                collect_agent_dependencies_from_expression(&object_field.value, referenced_agents);
            }

            for object_field in &tool_call.binding_fields {
                collect_agent_dependencies_from_expression(&object_field.value, referenced_agents);
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
    use crate::runtime::InferenceSetting;
    use crate::workflow_source;

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
                output: string
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

            tool search { query: string }
            tool search { query: string }

            agent researcher {}
            agent researcher {}
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProvider { provider_name } if provider_name == "openai",
            ValidationIssue::DuplicateSchema { schema_name } if schema_name == "User",
            ValidationIssue::DuplicateTool { tool_name } if tool_name == "search",
            ValidationIssue::DuplicateAgent { agent_name } if agent_name == "researcher"
        );
    }

    #[test]
    fn reports_unknown_tool_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                tools: [tool.web_search]
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownToolReference { tool_name, agent_name }
                if tool_name == "web_search" && agent_name == "researcher"
        );
    }

    #[test]
    fn accepts_declared_tool_reference() {
        let workflow = parse_inline_workflow! {
            tool web_search {
                query: string
            }

            agent researcher {
                tools: [tool.web_search]
            }
        };

        assert_workflow_issues_do_not_contain!(
            workflow,
            ValidationIssue::UnknownToolReference {
                tool_name: _,
                agent_name: _
            }
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
    fn reports_duplicate_properties_in_declarations_and_object_definitions() {
        let workflow = parse_inline_workflow! {
            provider ollama {
                driver: "ollama"
                models: ["qwen3.5:8b"]
                models: ["qwen3.5:14b"]
            }

            schema Greeting {
                message: string
                message: string
            }

            input {
                profile: {
                    id: string
                    id: string
                }
            }

            agent greeting {
                model: ollama("qwen3.5:8b")
                prompt: "hello"
                prompt: "welcome"
                inference: {
                    temperature: 0.2
                    temperature: 0.4
                }
                output: string
            }

            output {
                payload: {
                    status: "ok"
                    status: "ready"
                }
            }
        };

        let validation_report = validate_workflow(&workflow);
        let duplicate_property_issues = validation_report
            .issues()
            .iter()
            .filter(|validation_issue| matches!(validation_issue, ValidationIssue::DuplicateProperty { .. }))
            .count();

        assert!(duplicate_property_issues >= 6);

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "prompt" && *context == ValidationContext::Agent("greeting".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "models" && *context == ValidationContext::Provider("ollama".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "message" && *context == ValidationContext::Schema("Greeting".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "id" && *context == ValidationContext::Input,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "status" && *context == ValidationContext::Output
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
    fn reports_invalid_inference_setting_value_type() {
        let workflow = parse_inline_workflow! {
            agent writer {
                inference: {
                    temperature: 0.2
                    max_tokens: "2_000"
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name,
                inference_setting
            } if agent_name == "writer" && *inference_setting == InferenceSetting::MaxTokens
        );
    }

    #[test]
    fn rejects_unknown_agent_properties_at_parse_time() {
        let workflow_source = workflow_source! {
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

        let parse_result = parse_workflow(workflow_source);

        assert!(parse_result.is_err(), "unknown agent properties should fail parsing");
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
    fn unknown_agent_property_diagnostic_suggests_closest_property_name() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "prom_t".to_string(),
        };

        let diagnostic = issue.diagnostic(None);
        let help_message = diagnostic.help.expect("unknown property diagnostics should include help");

        assert!(help_message.contains("Did you mean `prompt`?"));
        assert!(help_message.contains("Supported properties:"));
    }

    #[test]
    fn unknown_agent_property_diagnostic_lists_supported_properties_without_guess() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "retries".to_string(),
        };

        let diagnostic = issue.diagnostic(None);
        let help_message = diagnostic.help.expect("unknown property diagnostics should include help");

        assert!(help_message.contains("Supported properties:"));
        assert!(!help_message.contains("Did you mean"));
    }

    #[test]
    fn all_validation_issue_diagnostics_include_recovery_help() {
        let validation_issues = vec![
            ValidationIssue::DuplicateProvider {
                provider_name: "openai".to_string(),
            },
            ValidationIssue::DuplicateSchema {
                schema_name: "Result".to_string(),
            },
            ValidationIssue::DuplicateAgent {
                agent_name: "writer".to_string(),
            },
            ValidationIssue::DuplicateSingletonDeclaration {
                declaration_kind: SingletonDeclarationKind::Input,
            },
            ValidationIssue::DuplicateProperty {
                property_name: "prompt".to_string(),
                context: ValidationContext::Agent("writer".to_string()),
            },
            ValidationIssue::UnknownAgentProperty {
                agent_name: "writer".to_string(),
                property_name: "prom_t".to_string(),
            },
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name: "writer".to_string(),
                inference_setting: InferenceSetting::MaxTokens,
            },
            ValidationIssue::InvalidModelExpression {
                agent_name: "writer".to_string(),
            },
            ValidationIssue::UnknownProviderInModel {
                agent_name: "writer".to_string(),
                provider_name: "missing_provider".to_string(),
            },
            ValidationIssue::UnknownModelForProvider {
                agent_name: "writer".to_string(),
                provider_name: "openai".to_string(),
                model_name: "gpt-unknown".to_string(),
            },
            ValidationIssue::UnknownAgentReference {
                referenced_agent: "missing_agent".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidKeywordReferenceRoot {
                keyword: ReferenceKeyword::Input,
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingInputDeclaration {
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingSecretsDeclaration {
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownInputFieldReference {
                field_name: "topic".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownSecretsFieldReference {
                field_name: "api_key".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::SecretReferenceInLlmContext {
                reference_path: "secrets.api_key".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name: "writer".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingOptionalReferenceAccess {
                reference_path: "agent.writer.payload.value".to_string(),
                field_name: "value".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidReferencePath {
                reference_path: "agent.writer.score".to_string(),
                invalid_field: "score".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidForLoopIterableType {
                agent_name: "analyzer".to_string(),
                found_type: "{ tasks: [{ id: number }] }".to_string(),
            },
            ValidationIssue::UnknownSchemaReference {
                referenced_schema: "MissingSchema".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::AgentDependencyCycle {
                agent_names: vec!["alpha".to_string(), "beta".to_string()],
            },
        ];

        for validation_issue in validation_issues {
            let diagnostic = validation_issue.diagnostic(None);
            let help_message = diagnostic.help.expect("validation diagnostic should include recovery help");

            assert!(!help_message.trim().is_empty());
        }
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
    fn reports_invalid_for_loop_iterable_type_for_object_reference() {
        let workflow = parse_inline_workflow! {
            agent summarizer {
                output: {
                    tasks: [{ id: number }]
                    participants: [{ id: number }]
                }
            }

            agent analyzer for participant in agent.summarizer {
                output: string
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidForLoopIterableType {
                agent_name,
                found_type: _
            } if agent_name == "analyzer"
        );
    }

    #[test]
    fn allows_for_loop_iterable_type_for_array_reference() {
        let workflow = parse_inline_workflow! {
            agent summarizer {
                output: {
                    tasks: [{ id: number }]
                    participants: [{ id: number }]
                }
            }

            agent analyzer for participant in agent.summarizer.participants {
                output: string
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidForLoopIterableType { .. });
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
    fn allows_dynamic_model_expression_without_literal_lookup() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            secrets {
                selected_model: string
            }

            agent researcher {
                model: openai(secrets.selected_model)
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidModelExpression { .. });
        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::UnknownModelForProvider { .. });
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
    fn reports_missing_agent_output_type_for_output_agent_reference() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                prompt: "Write a short welcome message."
            }

            output {
                greeting: agent.greeting
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name,
                context
            } if agent_name == "greeting" && *context == ValidationContext::Output
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
    fn reports_missing_optional_reference_access_for_nullable_agent_output_path() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output: {
                    nested: {
                        value: string
                    } | null
                }
            }

            output {
                greeting: agent.greeting.nested.value
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingOptionalReferenceAccess {
                reference_path,
                field_name,
                context
            } if reference_path == "agent.greeting.nested.value"
                && field_name == "value"
                && *context == ValidationContext::Output
        );
    }

    #[test]
    fn accepts_optional_reference_access_for_nullable_agent_output_path() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output: {
                    nested: {
                        value: string
                    } | null
                }
            }

            output {
                greeting: agent.greeting.nested?.value
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::MissingOptionalReferenceAccess { .. });
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
    fn reports_invalid_type_expression_reference_root() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output: test
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path,
                context
            } if reference_path == "test" && *context == ValidationContext::Agent("greeting".to_owned())
        );
    }

    #[test]
    fn reports_invalid_keyword_root_in_type_expression_reference() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path,
                context
            } if reference_path == "secrets.api_key" && *context == ValidationContext::Agent("greeting".to_owned())
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

    #[test]
    fn reports_duplicate_dynamic_fields_across_workflow_blocks() {
        let workflow = parse_inline_workflow! {
            dynamic {
                max_results: 5
            }

            dynamic {
                max_results: 10
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "max_results" && *context == ValidationContext::Dynamic
        );
    }

    #[test]
    fn reports_missing_dynamic_declaration_for_dynamic_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                prompt: dynamic.topic
                output: string
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingDynamicDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }
}
