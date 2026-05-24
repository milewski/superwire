use crate::error::ModelProviderError;
use crate::types::{ModelRequest, ModelResponse};
use async_trait::async_trait;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelProviderError>;
}
