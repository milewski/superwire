use super::{ProviderConfigParser, ProviderConfigTemplate};
use crate::dsl::{Expression, ProviderDeclaration};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::EvaluationContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAIProviderConfig {
    pub endpoint: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAIProviderConfigTemplate {
    pub endpoint_expression: Expression,
    pub api_key_expression: Expression,
}

impl OpenAIProviderConfigTemplate {
    pub fn resolve(
        &self,
        provider_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<OpenAIProviderConfig, WorkflowRuntimeError> {
        let endpoint = self
            .endpoint_expression
            .evaluate_as_provider_string(provider_name, "endpoint", evaluation_context)?;

        let api_key = self
            .api_key_expression
            .evaluate_as_provider_string(provider_name, "api_key", evaluation_context)?;

        Ok(OpenAIProviderConfig { endpoint, api_key })
    }
}

pub struct OpenAIProviderConfigParser;

impl ProviderConfigParser for OpenAIProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfigTemplate, WorkflowRuntimeError> {
        let endpoint_expression = Self::required_property_expression(provider_declaration, "endpoint")?;
        let api_key_expression = Self::required_property_expression(provider_declaration, "api_key")?;

        Ok(ProviderConfigTemplate::OpenAI(OpenAIProviderConfigTemplate {
            endpoint_expression,
            api_key_expression,
        }))
    }
}
