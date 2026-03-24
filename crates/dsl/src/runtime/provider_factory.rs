use crate::compiler::{CompiledProvider, ProviderDriver};
use crate::error::WorkflowError;
use engine_ai_agent::{OllamaProvider, OpenAIProvider, Provider};
use reqwest::Url;
use std::sync::Arc;

pub trait ProviderFactory: Send + Sync {
    fn build_provider(
        &self,
        agent_name: &str,
        provider: &CompiledProvider,
        model_name: &str,
        api_key: Option<&str>,
    ) -> Result<Arc<dyn Provider + Send + Sync>, WorkflowError>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultProviderFactory;

impl ProviderFactory for DefaultProviderFactory {
    fn build_provider(
        &self,
        _agent_name: &str,
        provider: &CompiledProvider,
        model_name: &str,
        api_key: Option<&str>,
    ) -> Result<Arc<dyn Provider + Send + Sync>, WorkflowError> {
        match provider.driver {
            ProviderDriver::Ollama => {
                let (host, port) = parse_ollama_endpoint(provider.endpoint.as_deref())?;

                Ok(Arc::new(OllamaProvider::new(host, port, model_name.to_string())))
            }
            ProviderDriver::OpenAi => {
                let base_url = provider.endpoint.clone().unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                let provider = OpenAIProvider::new_with_base_url(base_url, api_key.unwrap_or_default().to_string(), model_name.to_string());

                Ok(Arc::new(provider))
            }
        }
    }
}

fn parse_ollama_endpoint(endpoint: Option<&str>) -> Result<(String, u16), WorkflowError> {
    let endpoint = endpoint.unwrap_or("http://127.0.0.1:11434");
    let url = Url::parse(endpoint).map_err(|error| WorkflowError::execution(format!("invalid Ollama endpoint '{endpoint}': {error}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| WorkflowError::execution(format!("Ollama endpoint '{endpoint}' is missing a host")))?;
    let port = url.port_or_known_default().unwrap_or(11434);

    Ok((format!("{}://{host}", url.scheme()), port))
}
