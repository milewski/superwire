use superwire_mcp::McpError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelProviderError {
    #[error(transparent)]
    Mcp(#[from] McpError),

    #[error("agent `{agent_name}` model error: {message}")]
    Model { agent_name: String, message: String },

    #[error("{message}")]
    Other { message: String },
}
