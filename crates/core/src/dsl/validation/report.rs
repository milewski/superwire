use super::super::ast::{ReferenceKeyword, SourceSpan};
use super::super::structure;
use crate::diagnostic::should_render_rich_diagnostics;
use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::semantic::{InferenceSetting, ProviderDriver};

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

    #[must_use]
    pub fn render_for_output_target(&self, source_text: Option<&str>, source_name: &str) -> String {
        if should_render_rich_diagnostics() {
            if let Some(source_text) = source_text {
                return self.render_with_source(source_text, source_name);
            }
        }

        self.render()
    }

    pub(super) fn push_issue(&mut self, issue: ValidationIssue) {
        self.push_issue_with_span(issue, None);
    }

    pub(crate) fn push_issue_with_span(&mut self, issue: ValidationIssue, span: Option<SourceSpan>) {
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
    Model(String),
    Schema(String),
    Tool(String),
    Resource(String),
    Prompt(String),
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
            Self::Model(model_name) => format!("model `{model_name}`"),
            Self::Schema(schema_name) => format!("schema `{schema_name}`"),
            Self::Tool(tool_name) => format!("tool `{tool_name}`"),
            Self::Resource(resource_name) => format!("resource `{resource_name}`"),
            Self::Prompt(prompt_name) => format!("prompt `{prompt_name}`"),
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
    InvalidProviderName {
        provider_name: String,
    },
    UnknownProviderDriver {
        provider_name: String,
        driver_name: String,
    },
    DuplicateModel {
        model_name: String,
    },
    InvalidModelName {
        model_name: String,
    },
    UnknownProviderInModelDeclaration {
        model_name: String,
        provider_name: String,
    },
    MissingModelId {
        model_name: String,
    },
    UnknownModelProfile {
        agent_name: String,
        model_name: String,
    },
    InvalidModelUsageProperty {
        agent_name: String,
        property_name: String,
    },
    DuplicateSchema {
        schema_name: String,
    },
    InvalidSchemaName {
        schema_name: String,
    },
    InvalidVariantDiscriminatorField {
        discriminator: String,
        case_name: String,
    },
    DuplicateTool {
        tool_name: String,
    },
    DuplicateResource {
        resource_name: String,
    },
    DuplicatePrompt {
        prompt_name: String,
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
    UnknownResourceReference {
        resource_name: String,
        context: ValidationContext,
    },
    UnknownPromptReference {
        prompt_name: String,
        context: ValidationContext,
    },
    InvalidToolBinding {
        agent_name: String,
        tool_name: String,
        message: String,
    },
    InvalidTypeExpressionReference {
        reference_path: String,
        context: ValidationContext,
    },
    AgentDependencyCycle {
        agent_names: Vec<String>,
    },
    DynamicDependencyCycle {
        field_names: Vec<String>,
    },
}

impl ValidationIssue {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicateProvider { .. } => "duplicate_provider",
            Self::InvalidProviderName { .. } => "invalid_provider_name",
            Self::UnknownProviderDriver { .. } => "unknown_provider_driver",
            Self::DuplicateModel { .. } => "duplicate_model",
            Self::InvalidModelName { .. } => "invalid_model_name",
            Self::UnknownProviderInModelDeclaration { .. } => "unknown_provider_in_model_declaration",
            Self::MissingModelId { .. } => "missing_model_id",
            Self::UnknownModelProfile { .. } => "unknown_model_profile",
            Self::InvalidModelUsageProperty { .. } => "invalid_model_usage_property",
            Self::DuplicateSchema { .. } => "duplicate_schema",
            Self::InvalidSchemaName { .. } => "invalid_schema_name",
            Self::InvalidVariantDiscriminatorField { .. } => "invalid_variant_discriminator_field",
            Self::DuplicateTool { .. } => "duplicate_tool",
            Self::DuplicateResource { .. } => "duplicate_resource",
            Self::DuplicatePrompt { .. } => "duplicate_prompt",
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
            Self::UnknownResourceReference { .. } => "unknown_resource_reference",
            Self::UnknownPromptReference { .. } => "unknown_prompt_reference",
            Self::InvalidToolBinding { .. } => "invalid_tool_binding",
            Self::InvalidTypeExpressionReference { .. } => "invalid_type_expression_reference",
            Self::AgentDependencyCycle { .. } => "agent_dependency_cycle",
            Self::DynamicDependencyCycle { .. } => "dynamic_dependency_cycle",
        }
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn message(&self) -> String {
        match self {
            Self::DuplicateProvider { provider_name } => {
                format!("Provider `{provider_name}` is declared more than once.")
            }
            Self::InvalidProviderName { provider_name } => {
                format!("Provider `{provider_name}` must use lowercase snake_case.")
            }
            Self::UnknownProviderDriver {
                provider_name,
                driver_name,
            } => {
                format!("Provider `{provider_name}` references unknown driver `{driver_name}`.")
            }
            Self::DuplicateModel { model_name } => {
                format!("Model `{model_name}` is declared more than once.")
            }
            Self::InvalidModelName { model_name } => {
                format!("Model `{model_name}` must use lowercase snake_case.")
            }
            Self::UnknownProviderInModelDeclaration { model_name, provider_name } => {
                format!("Model `{model_name}` references unknown provider `{provider_name}`.")
            }
            Self::MissingModelId { model_name } => {
                format!("Model `{model_name}` must declare an `id` field.")
            }
            Self::UnknownModelProfile { agent_name, model_name } => {
                format!("Agent `{agent_name}` references unknown model profile `model.{model_name}`.")
            }
            Self::InvalidModelUsageProperty { agent_name, property_name } => {
                format!("Agent `{agent_name}` model usage block cannot override `{property_name}`.")
            }
            Self::DuplicateSchema { schema_name } => {
                format!("Schema `{schema_name}` is declared more than once.")
            }
            Self::InvalidSchemaName { schema_name } => {
                format!("Schema `{schema_name}` must use lowercase snake_case.")
            }
            Self::InvalidVariantDiscriminatorField { discriminator, case_name } => {
                format!("Variant case `{case_name}` must not manually declare discriminator field `{discriminator}`.")
            }
            Self::DuplicateTool { tool_name } => {
                format!("Tool `{tool_name}` is declared more than once.")
            }
            Self::DuplicateResource { resource_name } => {
                format!("Resource `{resource_name}` is imported more than once.")
            }
            Self::DuplicatePrompt { prompt_name } => {
                format!("Prompt `{prompt_name}` is imported more than once.")
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
            Self::UnknownResourceReference { resource_name, context } => {
                format!("Unknown resource `resource.{resource_name}` referenced in {}.", context.describe())
            }
            Self::UnknownPromptReference { prompt_name, context } => {
                format!("Unknown prompt `prompt.{prompt_name}` referenced in {}.", context.describe())
            }
            Self::InvalidToolBinding {
                agent_name,
                tool_name,
                message,
            } => {
                format!("Agent `{agent_name}` has invalid binding overrides for `tool.{tool_name}`: {message}.")
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
            Self::DynamicDependencyCycle { field_names } => {
                format!("Circular dynamic dependency detected: {}.", field_names.join(", "))
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
            | Self::DuplicateModel { .. }
            | Self::DuplicateSchema { .. }
            | Self::DuplicateTool { .. }
            | Self::DuplicateResource { .. }
            | Self::DuplicatePrompt { .. }
            | Self::DuplicateAgent { .. }
            | Self::DuplicateSingletonDeclaration { .. }
            | Self::DuplicateProperty { .. } => Some(self.duplicate_declaration_help_message()),
            Self::InvalidSchemaName { .. } => Some("Rename the schema using lowercase snake_case, such as `research_summary`.".to_string()),
            Self::InvalidVariantDiscriminatorField { discriminator, .. } => Some(format!(
                "Remove `{discriminator}` from the case body; the variant type injects this field automatically."
            )),
            Self::UnknownAgentProperty {
                agent_name: _,
                property_name,
            } => Some(Self::unknown_agent_property_help(property_name)),
            Self::InvalidProviderName { .. } => Some("Rename the provider using lowercase snake_case, such as `openai_cloud`.".to_string()),
            Self::InvalidModelName { .. } => Some("Rename the model using lowercase snake_case, such as `fast`.".to_string()),
            Self::UnknownProviderDriver { .. }
            | Self::UnknownProviderInModelDeclaration { .. }
            | Self::MissingModelId { .. }
            | Self::UnknownModelProfile { .. }
            | Self::InvalidModelUsageProperty { .. } => Some(self.agent_model_help_message()),
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
            | Self::UnknownResourceReference {
                resource_name: _,
                context: _,
            }
            | Self::UnknownPromptReference {
                prompt_name: _,
                context: _,
            }
            | Self::InvalidToolBinding {
                agent_name: _,
                tool_name: _,
                message: _,
            }
            | Self::InvalidTypeExpressionReference {
                reference_path: _,
                context: _,
            }
            | Self::AgentDependencyCycle { agent_names: _ }
            | Self::DynamicDependencyCycle { field_names: _ }
            | Self::MissingInputDeclaration { context: _ }
            | Self::MissingSecretsDeclaration { context: _ } => Some(self.reference_resolution_help_message()),
        }
    }

    fn duplicate_declaration_help_message(&self) -> String {
        match self {
            Self::DuplicateProvider { provider_name } => {
                format!("Keep a single `provider {provider_name} from ...` declaration, or rename one provider.")
            }
            Self::DuplicateModel { model_name } => {
                format!("Keep a single `model {model_name} from ...` declaration, or rename one model.")
            }
            Self::DuplicateSchema { schema_name } => {
                format!("Keep a single `schema {schema_name}` declaration, or rename one schema.")
            }
            Self::DuplicateTool { tool_name } => {
                format!("Keep a single `tool {tool_name}` declaration, or rename one tool.")
            }
            Self::DuplicateResource { resource_name } => {
                format!("Keep a single `resource {resource_name} from ...` import, or rename one resource.")
            }
            Self::DuplicatePrompt { prompt_name } => {
                format!("Keep a single `prompt {prompt_name} from ...` import, or rename one prompt.")
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
                "Use `model: model.<profile_name>` with a declared model profile.".to_string()
            }
            Self::UnknownProviderDriver {
                provider_name: _,
                driver_name,
            } => format!(
                "Use a registered provider driver such as `{}`, `{}`, `{}`, or `{}`.",
                ProviderDriver::OpenAi.as_str(),
                ProviderDriver::Anthropic.as_str(),
                ProviderDriver::Google.as_str(),
                ProviderDriver::OpenAiCompatible.as_str()
            )
            .replace(driver_name, driver_name),
            Self::UnknownProviderInModelDeclaration {
                model_name: _,
                provider_name,
            } => {
                format!("Declare `provider {provider_name} from <driver> {{ ... }}` before models that use it.")
            }
            Self::MissingModelId { model_name } => {
                format!("Add `id: \"provider-model-id\"` inside `model {model_name} from ...`.")
            }
            Self::UnknownModelProfile { agent_name: _, model_name } => {
                format!("Declare `model {model_name} from <provider> {{ id: \"...\" }}` or update the agent reference.")
            }
            Self::InvalidModelUsageProperty {
                agent_name: _,
                property_name: _,
            } => "Only `inference { ... }` is allowed inside an agent model usage block.".to_string(),
            Self::UnknownProviderInModel {
                agent_name: _,
                provider_name,
            } => {
                format!("Declare `provider {provider_name} from <driver> {{ ... }}` or update `model` to use an existing model profile.")
            }
            Self::UnknownModelForProvider {
                agent_name: _,
                provider_name,
                model_name,
            } => {
                format!("Declare a model profile for provider `{provider_name}` with `id: \"{model_name}\"`.")
            }
            _ => "Fix agent model and inference settings to use valid values.".to_string(),
        }
    }

    fn reference_resolution_help_message(&self) -> String {
        if let Some(help_message) = self.declaration_reference_help_message() {
            return help_message;
        }

        match self {
            Self::InvalidKeywordReferenceRoot { keyword, context: _ } => {
                format!("Add a field path after `{}`.", keyword.as_str())
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
                format!("Add `output {{ ... }}` to `agent {agent_name}` before referencing `agent.{agent_name}` or its fields.")
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
            Self::InvalidToolBinding {
                agent_name: _,
                tool_name: _,
                message: _,
            } => {
                "Pass required tool bindings with `tool.name { bindings { name: expression } }`, or make them fixed in the tool declaration."
                    .to_string()
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
            Self::DynamicDependencyCycle { field_names } => {
                format!(
                    "Break the cycle by removing at least one dependency among: {}.",
                    field_names.join(" -> ")
                )
            }
            _ => "Fix reference paths and declarations so all references resolve.".to_string(),
        }
    }

    fn declaration_reference_help_message(&self) -> Option<String> {
        match self {
            Self::UnknownAgentReference {
                referenced_agent,
                context: _,
            } => Some(format!(
                "Declare `agent {referenced_agent} {{ ... }}` before this reference, or fix the agent name."
            )),
            Self::MissingDynamicDeclaration { context: _ } => {
                Some("Add a `dynamic { ... }` block with the fields used by `dynamic.<field>` references.".to_string())
            }
            Self::MissingInputDeclaration { context: _ } => {
                Some("Add an `input { ... }` declaration with the fields used by `input.<field>` references.".to_string())
            }
            Self::MissingSecretsDeclaration { context: _ } => {
                Some("Add a `secrets { ... }` declaration with the fields used by `secrets.<field>` references.".to_string())
            }
            Self::UnknownInputFieldReference { field_name, context: _ } => Some(format!(
                "Add `{field_name}: <type>` to `input`, or reference an existing input field."
            )),
            Self::UnknownDynamicFieldReference { field_name, context: _ } => Some(format!(
                "Add `{field_name}: <value>` to a `dynamic` block, or reference an existing dynamic field."
            )),
            Self::UnknownSecretsFieldReference { field_name, context: _ } => Some(format!(
                "Add `{field_name}: <type>` to `secrets`, or reference an existing secrets field."
            )),
            Self::UnknownSchemaReference {
                referenced_schema,
                context: _,
            } => Some(format!(
                "Declare `schema {referenced_schema} {{ ... }}` before using `schema.{referenced_schema}`."
            )),
            Self::UnknownToolReference { tool_name, agent_name: _ } => Some(format!(
                "Declare `tool {tool_name} {{ ... }}` before using `tool.{tool_name}` in an agent `tools` list."
            )),
            Self::UnknownResourceReference { resource_name, context: _ } => Some(format!(
                "Import `resource {resource_name} from mcp.<server>.resource.<name>` before using `read resource.{resource_name}`."
            )),
            Self::UnknownPromptReference { prompt_name, context: _ } => Some(format!(
                "Import `prompt {prompt_name} from mcp.<server>.prompt.<name>` before using `render prompt.{prompt_name}`."
            )),
            _ => None,
        }
    }

    fn unknown_agent_property_help(property_name: &str) -> String {
        let agent = structure::Agent::new();

        if let Some(suggested_property_definition) = agent.suggested_property_definition(property_name) {
            return format!(
                "Did you mean `{}`? Supported properties: {}.",
                suggested_property_definition.name,
                agent.rendered_property_values()
            );
        }

        format!("Supported properties: {}.", agent.rendered_property_values())
    }
}

impl From<&ValidationIssue> for DiagnosticCode {
    #[allow(clippy::too_many_lines)]
    fn from(validation_issue: &ValidationIssue) -> Self {
        match validation_issue {
            ValidationIssue::DuplicateProvider { provider_name: _ } => Self::DuplicateProvider,
            ValidationIssue::InvalidProviderName { provider_name: _ } => Self::InvalidProviderName,
            ValidationIssue::UnknownProviderDriver {
                provider_name: _,
                driver_name: _,
            } => Self::UnknownProviderDriver,
            ValidationIssue::DuplicateModel { model_name: _ } => Self::DuplicateModel,
            ValidationIssue::InvalidModelName { model_name: _ } => Self::InvalidModelName,
            ValidationIssue::UnknownProviderInModelDeclaration {
                model_name: _,
                provider_name: _,
            } => Self::UnknownProviderInModelDeclaration,
            ValidationIssue::MissingModelId { model_name: _ } => Self::MissingModelId,
            ValidationIssue::UnknownModelProfile {
                agent_name: _,
                model_name: _,
            } => Self::UnknownModelProfile,
            ValidationIssue::InvalidModelUsageProperty {
                agent_name: _,
                property_name: _,
            } => Self::InvalidModelUsageProperty,
            ValidationIssue::DuplicateSchema { schema_name: _ } => Self::DuplicateSchema,
            ValidationIssue::InvalidSchemaName { schema_name: _ } => Self::InvalidSchemaName,
            ValidationIssue::InvalidVariantDiscriminatorField {
                discriminator: _,
                case_name: _,
            } => Self::InvalidVariantDiscriminatorField,
            ValidationIssue::DuplicateTool { tool_name: _ } => Self::DuplicateTool,
            ValidationIssue::DuplicateResource { resource_name: _ } => Self::DuplicateResource,
            ValidationIssue::DuplicatePrompt { prompt_name: _ } => Self::DuplicatePrompt,
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
            ValidationIssue::UnknownResourceReference {
                resource_name: _,
                context: _,
            } => Self::UnknownResourceReference,
            ValidationIssue::UnknownPromptReference {
                prompt_name: _,
                context: _,
            } => Self::UnknownPromptReference,
            ValidationIssue::InvalidToolBinding {
                agent_name: _,
                tool_name: _,
                message: _,
            } => Self::InvalidToolBinding,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path: _,
                context: _,
            } => Self::InvalidTypeExpressionReference,
            ValidationIssue::AgentDependencyCycle { agent_names: _ } => Self::AgentDependencyCycle,
            ValidationIssue::DynamicDependencyCycle { field_names: _ } => Self::DynamicDependencyCycle,
        }
    }
}
