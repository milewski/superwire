use crate::ast::Provider as AstProvider;
use crate::providers::error::ProviderError;
use crate::providers::ollama::OllamaProvider;
use crate::providers::provider::ProviderRef;
use std::sync::Arc;

pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create_provider(provider: &AstProvider) -> Result<ProviderRef, ProviderError> {
        match provider.driver.as_str() {
            "ollama" => {
                let api_endpoint = provider
                    .api_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".to_string());

                let ollama_provider = OllamaProvider::new(provider.name.clone(), api_endpoint, provider.models.clone());

                Ok(Arc::new(ollama_provider))
            }
            _ => Err(ProviderError::ConnectionError {
                message: format!("Unknown provider driver: {}", provider.driver),
                suggestion: Some("Supported drivers: ollama".to_string()),
            }),
        }
    }
}
