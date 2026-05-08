#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("MCP declaration `{server_name}` requires string property `endpoint`")]
    MissingEndpoint { server_name: String },

    #[error("MCP declaration `{server_name}` property `{property_name}` must be {expected}")]
    InvalidProperty {
        server_name: String,
        property_name: String,
        expected: &'static str,
    },

    #[error("MCP declaration `{server_name}` property `{property_name}` could not be resolved: {reason}")]
    InvalidPropertyEvaluation {
        server_name: String,
        property_name: String,
        reason: String,
    },

    #[error("MCP server `{server_name}` HTTP request for `{method}` failed: {message}")]
    Http {
        server_name: String,
        method: String,
        message: String,
    },

    #[error("MCP server `{server_name}` returned an error for `{method}`: {message}")]
    Rpc {
        server_name: String,
        method: String,
        message: String,
    },

    #[error("MCP server `{server_name}` response for `{method}` did not include a result")]
    MissingResult { server_name: String, method: String },

    #[error("MCP server `{server_name}` response for `{method}` did not match MCP schema: {message}")]
    InvalidResponse {
        server_name: String,
        method: String,
        message: String,
    },

    #[error("failed to read MCP lock `{path}`: {source}")]
    ReadLock { path: String, source: std::io::Error },

    #[error("failed to parse MCP lock `{path}`: {source}")]
    ParseLock { path: String, source: serde_json::Error },

    #[error("failed to write MCP lock `{path}`: {source}")]
    WriteLock { path: String, source: std::io::Error },

    #[error("failed to serialize MCP lock `{path}`: {source}")]
    SerializeLock { path: String, source: serde_json::Error },
}
