use super::{ProviderConfigParser, ProviderConfigTemplate};
use crate::dsl::{Expression, ProviderDeclaration};
use crate::semantic::support::expression::EvaluationContext;
use crate::semantic::WorkflowSemanticError;
use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaProviderConfig {
    pub endpoint: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaProviderConfigTemplate {
    pub endpoint_expression: Expression,
}

impl OllamaProviderConfigTemplate {
    pub fn resolve(
        &self,
        provider_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<OllamaProviderConfig, WorkflowSemanticError> {
        let endpoint = self
            .endpoint_expression
            .evaluate_as_provider_string(provider_name, "endpoint", evaluation_context)?;

        let parsed_endpoint = Url::parse(&endpoint).map_err(|error| WorkflowSemanticError::ProviderConfiguration {
            provider_name: provider_name.to_string(),
            message: format!("invalid ollama endpoint `{endpoint}`: {error}"),
        })?;

        let Some(host_name) = parsed_endpoint.host_str() else {
            return Err(WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_name.to_string(),
                message: format!("ollama endpoint is missing host: `{endpoint}`"),
            });
        };

        let Some(port) = parsed_endpoint.port() else {
            return Err(WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_name.to_string(),
                message: format!("ollama endpoint must include an explicit port: `{endpoint}`"),
            });
        };

        let host = format!("{}://{host_name}", parsed_endpoint.scheme());

        Ok(OllamaProviderConfig { endpoint, host, port })
    }
}

pub struct OllamaProviderConfigParser;

impl ProviderConfigParser for OllamaProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfigTemplate, WorkflowSemanticError> {
        let endpoint_expression = Self::required_property_expression(provider_declaration, "endpoint")?;

        Ok(ProviderConfigTemplate::Ollama(OllamaProviderConfigTemplate { endpoint_expression }))
    }
}
