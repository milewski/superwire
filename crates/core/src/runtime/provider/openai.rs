use super::{ProviderConfig, ProviderConfigParser};
use crate::dsl::ProviderDeclaration;
use crate::runtime::error::WorkflowRuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAIProviderConfig {
    pub endpoint: String,
    pub api_key: String,
}

pub struct OpenAIProviderConfigParser;

impl ProviderConfigParser for OpenAIProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfig, WorkflowRuntimeError> {
        let endpoint = Self::required_string_property(provider_declaration, "endpoint")?;
        let api_key = Self::required_string_property(provider_declaration, "api_key")?;

        Ok(ProviderConfig::OpenAI(OpenAIProviderConfig { endpoint, api_key }))
    }
}
