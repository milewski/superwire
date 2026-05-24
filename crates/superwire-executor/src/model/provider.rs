use crate::model::types::{ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use async_trait::async_trait;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError>;
}
