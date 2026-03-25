use super::{ProviderConfig, ProviderConfigParser};
use crate::dsl::ProviderDeclaration;
use crate::runtime::error::WorkflowRuntimeError;
use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaProviderConfig {
    pub endpoint: String,
    pub host: String,
    pub port: u16,
}

pub struct OllamaProviderConfigParser;

impl ProviderConfigParser for OllamaProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfig, WorkflowRuntimeError> {
        let endpoint = Self::required_string_property(provider_declaration, "endpoint")?;

        let parsed_endpoint = Url::parse(&endpoint).map_err(|error| WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("invalid ollama endpoint `{endpoint}`: {error}"),
        })?;

        let Some(host_name) = parsed_endpoint.host_str() else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("ollama endpoint is missing host: `{endpoint}`"),
            });
        };

        let Some(port) = parsed_endpoint.port() else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("ollama endpoint must include an explicit port: `{endpoint}`"),
            });
        };

        let host = format!("{}://{host_name}", parsed_endpoint.scheme());

        Ok(ProviderConfig::Ollama(OllamaProviderConfig { endpoint, host, port }))
    }
}
