use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiagnosticCode {
    #[serde(rename = "parse_error")]
    ParseError,
    #[serde(rename = "missing_node")]
    MissingNode,
    #[serde(rename = "unexpected_rule")]
    UnexpectedRule,
    #[serde(rename = "invalid_integer_literal")]
    InvalidIntegerLiteral,
    #[serde(rename = "duplicate_provider")]
    DuplicateProvider,
    #[serde(rename = "invalid_provider_name")]
    InvalidProviderName,
    #[serde(rename = "unknown_provider_driver")]
    UnknownProviderDriver,
    #[serde(rename = "duplicate_model")]
    DuplicateModel,
    #[serde(rename = "invalid_model_name")]
    InvalidModelName,
    #[serde(rename = "unknown_provider_in_model_declaration")]
    UnknownProviderInModelDeclaration,
    #[serde(rename = "missing_model_id")]
    MissingModelId,
    #[serde(rename = "unknown_model_profile")]
    UnknownModelProfile,
    #[serde(rename = "invalid_model_usage_property")]
    InvalidModelUsageProperty,
    #[serde(rename = "duplicate_schema")]
    DuplicateSchema,
    #[serde(rename = "invalid_schema_name")]
    InvalidSchemaName,
    #[serde(rename = "invalid_variant_discriminator_field")]
    InvalidVariantDiscriminatorField,
    #[serde(rename = "duplicate_tool")]
    DuplicateTool,
    #[serde(rename = "duplicate_resource")]
    DuplicateResource,
    #[serde(rename = "duplicate_prompt")]
    DuplicatePrompt,
    #[serde(rename = "duplicate_agent")]
    DuplicateAgent,
    #[serde(rename = "duplicate_singleton_declaration")]
    DuplicateSingletonDeclaration,
    #[serde(rename = "duplicate_property")]
    DuplicateProperty,
    #[serde(rename = "unknown_agent_property")]
    UnknownAgentProperty,
    #[serde(rename = "invalid_inference_setting_value_type")]
    InvalidInferenceSettingValueType,
    #[serde(rename = "invalid_model_expression")]
    InvalidModelExpression,
    #[serde(rename = "unknown_provider_in_model")]
    UnknownProviderInModel,
    #[serde(rename = "unknown_model_for_provider")]
    UnknownModelForProvider,
    #[serde(rename = "unknown_agent_reference")]
    UnknownAgentReference,
    #[serde(rename = "invalid_keyword_reference_root")]
    InvalidKeywordReferenceRoot,
    #[serde(rename = "missing_dynamic_declaration")]
    MissingDynamicDeclaration,
    #[serde(rename = "missing_input_declaration")]
    MissingInputDeclaration,
    #[serde(rename = "missing_secrets_declaration")]
    MissingSecretsDeclaration,
    #[serde(rename = "unknown_dynamic_field_reference")]
    UnknownDynamicFieldReference,
    #[serde(rename = "unknown_input_field_reference")]
    UnknownInputFieldReference,
    #[serde(rename = "unknown_secrets_field_reference")]
    UnknownSecretsFieldReference,
    #[serde(rename = "secret_reference_in_llm_context")]
    SecretReferenceInLlmContext,
    #[serde(rename = "missing_agent_output_type_for_field_reference")]
    MissingAgentOutputTypeForFieldReference,
    #[serde(rename = "missing_optional_reference_access")]
    MissingOptionalReferenceAccess,
    #[serde(rename = "invalid_reference_path")]
    InvalidReferencePath,
    #[serde(rename = "invalid_for_loop_iterable_type")]
    InvalidForLoopIterableType,
    #[serde(rename = "unknown_schema_reference")]
    UnknownSchemaReference,
    #[serde(rename = "unknown_tool_reference")]
    UnknownToolReference,
    #[serde(rename = "unknown_resource_reference")]
    UnknownResourceReference,
    #[serde(rename = "unknown_prompt_reference")]
    UnknownPromptReference,
    #[serde(rename = "invalid_tool_binding")]
    InvalidToolBinding,
    #[serde(rename = "invalid_type_expression_reference")]
    InvalidTypeExpressionReference,
    #[serde(rename = "agent_dependency_cycle")]
    AgentDependencyCycle,
    #[serde(rename = "dynamic_dependency_cycle")]
    DynamicDependencyCycle,
    #[serde(rename = "workflow_compilation_error")]
    WorkflowCompilationError,
}

impl DiagnosticCode {
    #[must_use]
    pub fn as_lsp_code(self) -> lsp_types::NumberOrString {
        let code = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| format!("{self:?}"));

        lsp_types::NumberOrString::String(code)
    }
}
