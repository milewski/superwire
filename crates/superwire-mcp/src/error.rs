use crate::{McpBlockingOperation, McpNetworkPolicy};

use serde_json::Value;

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

    #[error("MCP server `{server_name}` is blocked by `{policy}` network policy: {message}")]
    NetworkPolicyViolation {
        server_name: String,
        policy: McpNetworkPolicy,
        message: String,
    },

    #[error("MCP endpoint `{server_name}` was not approved for the current workflow request")]
    EndpointNotApproved { server_name: String },

    #[error("MCP endpoint approval does not match server `{server_name}` configuration")]
    EndpointApprovalMismatch { server_name: String },

    #[error("MCP endpoint approval for server `{server_name}` has expired")]
    EndpointApprovalExpired { server_name: String },

    #[error("MCP blocking executor is saturated while trying to {operation}")]
    BlockingExecutorSaturated { operation: McpBlockingOperation },

    #[error("MCP blocking executor is unavailable while trying to {operation}: {message}")]
    BlockingExecutorUnavailable { operation: McpBlockingOperation, message: String },

    #[error("MCP blocking operation `{operation}` exceeded its completion bound")]
    BlockingOperationTimedOut { operation: McpBlockingOperation },

    #[error("MCP blocking operation `{operation}` was cancelled before dispatch")]
    BlockingOperationCancelled { operation: McpBlockingOperation },

    #[error("MCP blocking operation `{operation}` has insufficient caller lifetime for its mandatory request bound")]
    BlockingOperationDeadlineInsufficient { operation: McpBlockingOperation },

    #[error("MCP blocking operation `{operation}` panicked")]
    BlockingOperationPanicked { operation: McpBlockingOperation },

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

    #[error("MCP server `{server_name}` tool `{tool_name}` returned an error: {message}")]
    ToolCallFailed {
        server_name: String,
        tool_name: String,
        message: String,
        detail: Value,
    },

    #[error("MCP server `{server_name}` resource `{resource_name}` arguments are invalid: {message}")]
    InvalidResourceArguments {
        server_name: String,
        resource_name: String,
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

impl McpError {
    #[must_use]
    pub fn is_http_status(&self, status_code: u16) -> bool {
        let Self::Http { message, .. } = self else {
            return false;
        };

        message.contains(&format!("status: {status_code}"))
    }

    #[must_use]
    pub const fn is_network_policy_violation(&self) -> bool {
        matches!(self, Self::NetworkPolicyViolation { .. })
    }

    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            Self::MissingEndpoint { server_name } => {
                format!("MCP declaration `{server_name}` requires an `endpoint`")
            }
            Self::InvalidProperty {
                server_name,
                property_name,
                expected,
            } => format!("MCP declaration `{server_name}` property `{property_name}` must be {expected}"),
            Self::InvalidPropertyEvaluation {
                server_name,
                property_name,
                ..
            } => format!("MCP declaration `{server_name}` property `{property_name}` could not be resolved"),
            Self::NetworkPolicyViolation {
                server_name,
                policy,
                message,
            } => format!("MCP server `{server_name}` is blocked by `{policy}` network policy: {message}"),
            Self::EndpointNotApproved { server_name } => {
                format!("MCP endpoint `{server_name}` was not approved for the current workflow request")
            }
            Self::EndpointApprovalMismatch { server_name } => {
                format!("MCP endpoint approval does not match server `{server_name}` configuration")
            }
            Self::EndpointApprovalExpired { server_name } => {
                format!("MCP endpoint approval for server `{server_name}` has expired")
            }
            Self::BlockingExecutorSaturated { operation } => {
                format!("MCP capacity is exhausted while trying to {operation}")
            }
            Self::BlockingExecutorUnavailable { operation, .. } => {
                format!("MCP execution is unavailable while trying to {operation}")
            }
            Self::BlockingOperationTimedOut { operation } => {
                format!("MCP operation `{operation}` exceeded its completion bound")
            }
            Self::BlockingOperationCancelled { operation } => {
                format!("MCP operation `{operation}` was cancelled before dispatch")
            }
            Self::BlockingOperationDeadlineInsufficient { operation } => {
                format!("MCP operation `{operation}` cannot start within its caller deadline")
            }
            Self::BlockingOperationPanicked { operation } => {
                format!("MCP operation `{operation}` failed internally")
            }
            Self::Http { server_name, method, .. } => format!("MCP server `{server_name}` HTTP request for `{method}` failed"),
            Self::Rpc { server_name, method, .. } => format!("MCP server `{server_name}` returned an error for `{method}`"),
            Self::ToolCallFailed {
                server_name, tool_name, ..
            } => format!("MCP server `{server_name}` tool `{tool_name}` returned an error"),
            Self::InvalidResourceArguments {
                server_name,
                resource_name,
                ..
            } => format!("MCP server `{server_name}` resource `{resource_name}` arguments are invalid"),
            Self::MissingResult { server_name, method } => {
                format!("MCP server `{server_name}` response for `{method}` did not include a result")
            }
            Self::InvalidResponse { server_name, method, .. } => {
                format!("MCP server `{server_name}` response for `{method}` did not match MCP schema")
            }
            Self::ReadLock { .. } => "failed to read MCP lock".to_string(),
            Self::ParseLock { .. } => "failed to parse MCP lock".to_string(),
            Self::WriteLock { .. } => "failed to write MCP lock".to_string(),
            Self::SerializeLock { .. } => "failed to serialize MCP lock".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_messages_omit_remote_payload_details() {
        const SECRET_SENTINEL: &str = "superwire-secret-sentinel";
        let errors = [
            McpError::InvalidPropertyEvaluation {
                server_name: "local".to_string(),
                property_name: "headers".to_string(),
                reason: SECRET_SENTINEL.to_string(),
            },
            McpError::Http {
                server_name: "local".to_string(),
                method: "tools/call".to_string(),
                message: SECRET_SENTINEL.to_string(),
            },
            McpError::Rpc {
                server_name: "local".to_string(),
                method: "tools/call".to_string(),
                message: SECRET_SENTINEL.to_string(),
            },
            McpError::ToolCallFailed {
                server_name: "local".to_string(),
                tool_name: "search".to_string(),
                message: SECRET_SENTINEL.to_string(),
                detail: serde_json::json!({ "secret": SECRET_SENTINEL }),
            },
        ];

        for error in errors {
            let public_message = error.public_message();

            assert!(!public_message.contains(SECRET_SENTINEL));
            assert!(public_message.contains("local"));
        }
    }
}
