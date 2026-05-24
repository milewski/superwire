use super::super::ast::{
    AgentDeclaration, AgentProperty, ObjectField, Reference, ReferenceAccess, ReferenceKeyword, TypedField, VariantCase,
};
use super::{ValidationContext, ValidationIssue};

pub(super) trait AgentDeclarationIssuesExt {
    fn invalid_model_expression_issue(&self) -> ValidationIssue;

    fn unknown_model_profile_issue(&self, model_name: &str) -> ValidationIssue;

    fn invalid_model_usage_property_issue(&self, property_name: &str) -> ValidationIssue;

    fn unknown_tool_reference_issue(&self, tool_name: &str) -> ValidationIssue;

    fn unknown_prompt_reference_issue(&self, prompt_name: &str) -> ValidationIssue;

    fn unknown_resource_reference_issue(&self, resource_name: &str) -> ValidationIssue;

    fn unknown_property_issue(&self, property_name: &str) -> ValidationIssue;

    fn invalid_for_loop_iterable_type_issue(&self, found_type: String) -> ValidationIssue;

    fn invalid_for_loop_destructuring_binding_issue(&self, binding_name: &str) -> ValidationIssue;

    fn invalid_tool_binding_issue(&self, tool_name: &str, message: String) -> ValidationIssue;
}

impl AgentDeclarationIssuesExt for AgentDeclaration {
    fn invalid_model_expression_issue(&self) -> ValidationIssue {
        ValidationIssue::InvalidModelExpression {
            agent_name: self.name.clone(),
        }
    }

    fn unknown_model_profile_issue(&self, model_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownModelProfile {
            agent_name: self.name.clone(),
            model_name: model_name.to_owned(),
        }
    }

    fn invalid_model_usage_property_issue(&self, property_name: &str) -> ValidationIssue {
        ValidationIssue::InvalidModelUsageProperty {
            agent_name: self.name.clone(),
            property_name: property_name.to_owned(),
        }
    }

    fn unknown_tool_reference_issue(&self, tool_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownToolReference {
            agent_name: self.name.clone(),
            tool_name: tool_name.to_owned(),
        }
    }

    fn unknown_prompt_reference_issue(&self, prompt_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownPromptReference {
            prompt_name: prompt_name.to_owned(),
            context: ValidationContext::Agent(self.name.clone()),
        }
    }

    fn unknown_resource_reference_issue(&self, resource_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownResourceReference {
            resource_name: resource_name.to_owned(),
            context: ValidationContext::Agent(self.name.clone()),
        }
    }

    fn unknown_property_issue(&self, property_name: &str) -> ValidationIssue {
        ValidationIssue::UnknownAgentProperty {
            agent_name: self.name.clone(),
            property_name: property_name.to_owned(),
        }
    }

    fn invalid_for_loop_iterable_type_issue(&self, found_type: String) -> ValidationIssue {
        ValidationIssue::InvalidForLoopIterableType {
            agent_name: self.name.clone(),
            found_type,
        }
    }

    fn invalid_for_loop_destructuring_binding_issue(&self, binding_name: &str) -> ValidationIssue {
        ValidationIssue::InvalidForLoopDestructuringBinding {
            agent_name: self.name.clone(),
            binding_name: binding_name.to_owned(),
        }
    }

    fn invalid_tool_binding_issue(&self, tool_name: &str, message: String) -> ValidationIssue {
        ValidationIssue::InvalidToolBinding {
            agent_name: self.name.clone(),
            tool_name: tool_name.to_owned(),
            message,
        }
    }
}

pub(super) trait ReferenceIssuesExt {
    fn invalid_keyword_root_issue(reference_keyword: ReferenceKeyword, context: ValidationContext) -> ValidationIssue;

    fn missing_dynamic_declaration_issue(context: ValidationContext) -> ValidationIssue;

    fn missing_input_declaration_issue(context: ValidationContext) -> ValidationIssue;

    fn missing_secrets_declaration_issue(context: ValidationContext) -> ValidationIssue;

    fn unknown_agent_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_dynamic_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_input_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_local_binding_reference_issue(binding_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_secrets_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_resource_reference_issue(resource_name: &str, context: ValidationContext) -> ValidationIssue;

    fn unknown_prompt_reference_issue(prompt_name: &str, context: ValidationContext) -> ValidationIssue;

    fn missing_agent_output_type_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue;

    fn missing_optional_access_issue(&self, field_name: &str, context: ValidationContext) -> ValidationIssue;

    fn invalid_path_issue(&self, reference_access: &ReferenceAccess, context: ValidationContext) -> ValidationIssue;

    fn secret_reference_in_llm_context_issue(&self, context: ValidationContext) -> ValidationIssue;

    fn invalid_type_expression_reference_issue(&self, context: ValidationContext) -> ValidationIssue;
}

impl ReferenceIssuesExt for Reference {
    fn invalid_keyword_root_issue(reference_keyword: ReferenceKeyword, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidKeywordReferenceRoot {
            keyword: reference_keyword,
            context,
        }
    }

    fn missing_dynamic_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingDynamicDeclaration { context }
    }

    fn missing_input_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingInputDeclaration { context }
    }

    fn missing_secrets_declaration_issue(context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingSecretsDeclaration { context }
    }

    fn unknown_agent_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownAgentReference {
            referenced_agent: referenced_agent_name.to_owned(),
            context,
        }
    }

    fn unknown_dynamic_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownDynamicFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    fn unknown_input_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownInputFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    fn unknown_local_binding_reference_issue(binding_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownLocalBindingReference {
            binding_name: binding_name.to_owned(),
            context,
        }
    }

    fn unknown_secrets_field_reference_issue(field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownSecretsFieldReference {
            field_name: field_name.to_owned(),
            context,
        }
    }

    fn unknown_resource_reference_issue(resource_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownResourceReference {
            resource_name: resource_name.to_owned(),
            context,
        }
    }

    fn unknown_prompt_reference_issue(prompt_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::UnknownPromptReference {
            prompt_name: prompt_name.to_owned(),
            context,
        }
    }

    fn missing_agent_output_type_reference_issue(referenced_agent_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingAgentOutputTypeForFieldReference {
            agent_name: referenced_agent_name.to_owned(),
            context,
        }
    }

    fn missing_optional_access_issue(&self, field_name: &str, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::MissingOptionalReferenceAccess {
            reference_path: self.render_path(),
            field_name: field_name.to_owned(),
            context,
        }
    }

    fn invalid_path_issue(&self, reference_access: &ReferenceAccess, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidReferencePath {
            reference_path: self.render_path(),
            invalid_field: reference_access.field.clone(),
            context,
        }
    }

    fn secret_reference_in_llm_context_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::SecretReferenceInLlmContext {
            reference_path: self.render_path(),
            context,
        }
    }

    fn invalid_type_expression_reference_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::InvalidTypeExpressionReference {
            reference_path: self.render_path(),
            context,
        }
    }
}

pub(super) trait ObjectFieldIssuesExt {
    fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue;
}

impl ObjectFieldIssuesExt for ObjectField {
    fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::DuplicateProperty {
            property_name: self.name.clone(),
            context,
        }
    }
}

pub(super) trait AgentPropertyIssuesExt {
    fn duplicate_property_issue(&self, context: ValidationContext) -> Option<ValidationIssue>;
}

impl AgentPropertyIssuesExt for AgentProperty {
    fn duplicate_property_issue(&self, context: ValidationContext) -> Option<ValidationIssue> {
        Some(ValidationIssue::DuplicateProperty {
            property_name: self.name()?.to_owned(),
            context,
        })
    }
}

pub(super) trait TypedFieldIssuesExt {
    fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue;
}

impl TypedFieldIssuesExt for TypedField {
    fn duplicate_property_issue(&self, context: ValidationContext) -> ValidationIssue {
        ValidationIssue::DuplicateProperty {
            property_name: self.name.clone(),
            context,
        }
    }
}

pub(super) trait VariantCaseIssuesExt {
    fn duplicate_discriminator_field_issue(&self, discriminator: &str) -> ValidationIssue;
}

impl VariantCaseIssuesExt for VariantCase {
    fn duplicate_discriminator_field_issue(&self, discriminator: &str) -> ValidationIssue {
        ValidationIssue::InvalidVariantDiscriminatorField {
            discriminator: discriminator.to_owned(),
            case_name: self.name.clone(),
        }
    }
}
