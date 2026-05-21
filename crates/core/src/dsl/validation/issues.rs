use super::super::ast::{
    AgentDeclaration, AgentProperty, ObjectField, Reference, ReferenceAccess, ReferenceKeyword, TypedField, VariantCase,
};
use super::report::{ValidationContext, ValidationIssue};

impl AgentDeclaration {
    pub(super) fn invalid_model_expression_issue(&self) -> ValidationIssue {
        ValidationIssue::InvalidModelExpression {
            agent_name: self.name.clone(),
        }
    }

    pub(super) fn unknown_model_profile_issue(&self, model_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownModelProfile {
            agent_name: self.name.clone(),
            model_name: model_name.to_owned(),
        }
    }

    pub(super) fn invalid_model_usage_property_issue(&self, property_name: &str) -> ValidationIssue {
        ValidationIssue::InvalidModelUsageProperty {
            agent_name: self.name.clone(),
            property_name: property_name.to_owned(),
        }
    }

    pub(super) fn unknown_tool_reference_issue(&self, tool_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownToolReference {
            agent_name: self.name.clone(),
            tool_name: tool_name.to_owned(),
        }
    }

    pub(super) fn unknown_prompt_reference_issue(&self, prompt_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownPromptReference {
            prompt_name: prompt_name.to_owned(),
            context: ValidationContext::Agent(self.name.clone()),
        }
    }

    pub(super) fn unknown_resource_reference_issue(&self, resource_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownResourceReference {
            resource_name: resource_name.to_owned(),
            context: ValidationContext::Agent(self.name.clone()),
        }
    }

    pub(super) fn unknown_property_issue(&self, property_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownAgentProperty {
            agent_name: self.name.clone(),
            property_name: property_name.to_owned(),
        }
    }

    pub(super) fn invalid_for_loop_iterable_type_issue(&self, found_type: String) -> ValidationIssue {
        ValidationIssue::InvalidForLoopIterableType {
            agent_name: self.name.clone(),
            found_type,
        }
    }

    pub(super) fn invalid_tool_binding_issue(&self, tool_name: &str, message: String) -> ValidationIssue {
        ValidationIssue::InvalidToolBinding {
            agent_name: self.name.clone(),
            tool_name: tool_name.to_owned(),
            message,
        }
    }
}

impl Reference {
    pub(super) fn invalid_keyword_root_issue(reference_keyword: ReferenceKeyword, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidKeywordReferenceRoot {
            keyword: reference_keyword,
            context,
        }
    }

    pub(super) fn missing_dynamic_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingDynamicDeclaration { context }
    }

    pub(super) fn missing_input_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingInputDeclaration { context }
    }

    pub(super) fn missing_secrets_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingSecretsDeclaration { context }
    }

    pub(super) fn unknown_agent_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownAgentReference {
            referenced_agent: referenced_agent_name.to_owned(),
            context,
        }
    }

    pub(super) fn unknown_dynamic_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownDynamicFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    pub(super) fn unknown_input_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownInputFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    pub(super) fn unknown_secrets_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownSecretsFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    pub(super) fn unknown_resource_reference_issue(resource_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownResourceReference {
            resource_name: resource_name.to_owned(),
            context,
        }
    }

    pub(super) fn unknown_prompt_reference_issue(prompt_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownPromptReference {
            prompt_name: prompt_name.to_owned(),
            context,
        }
    }

    pub(super) fn missing_agent_output_type_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingAgentOutputTypeForFieldReference {
            agent_name: referenced_agent_name.to_owned(),
            context,
        }
    }

    pub(super) fn missing_optional_access_issue(&self, field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingOptionalReferenceAccess {
            reference_path: self.render_path(),
            field_name: field_name.to_owned(),
            context,
        }
    }

    pub(super) fn invalid_path_issue(&self, reference_access: &ReferenceAccess, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidReferencePath {
            reference_path: self.render_path(),
            invalid_field: reference_access.field.clone(),
            context,
        }
    }

    pub(super) fn secret_reference_in_llm_context_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::SecretReferenceInLlmContext {
            reference_path: self.render_path(),
            context,
        }
    }

    pub(super) fn invalid_type_expression_reference_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidTypeExpressionReference {
            reference_path: self.render_path(),
            context,
        }
    }
}

impl ObjectField {
    pub(super) fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::DuplicateProperty {
            property_name: self.name.clone(),
            context,
        }
    }
}

impl AgentProperty {
    pub(super) fn duplicate_property_issue(&self, context: ValidationContext) -> Option<ValidationIssue> {
        Some(ValidationIssue::DuplicateProperty {
            property_name: self.name()?.to_owned(),
            context,
        })
    }
}

impl TypedField {
    pub(super) fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::DuplicateProperty {
            property_name: self.name.clone(),
            context,
        }
    }
}

impl VariantCase {
    pub(super) fn duplicate_discriminator_field_issue(&self, discriminator: &str) -> ValidationIssue {
        ValidationIssue::InvalidVariantDiscriminatorField {
            discriminator: discriminator.to_owned(),
            case_name: self.name.clone(),
        }
    }
}
