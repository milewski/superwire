use serde_json::Value;
use superwire_core::semantic::support::provider::OpenAIProviderConfig;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub agent_name: String,
    pub provider_config: OpenAIProviderConfig,
    pub model_name: String,
    pub prompt: String,
    pub output_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub output: Value,
    pub context: Value,
}
