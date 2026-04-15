//! Superwire tool system runtime core types and traits
//!
//! This module provides the foundational abstractions for the cross-platform tool system.
//! Tools can be executed via multiple backends (Wasm, native Rust, CLI) while maintaining
//! a consistent interface for introspection and execution.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Core error types for tool operations
#[derive(Debug, ThisError)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool introspection failed: {0}")]
    IntrospectionFailed(String),

    #[error("Invalid tool descriptor: {0}")]
    InvalidDescriptor(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Schema validation error: {0}")]
    SchemaValidationError(String),

    #[error("Backend error: {0}")]
    BackendError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Schema version for tool descriptors
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaVersion {
    #[serde(rename = "superwire.tool.v1")]
    V1,
}

/// Configuration for tool capability restrictions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCapabilities {
    #[serde(default)]
    pub network_access: bool,

    #[serde(default)]
    pub filesystem_access: bool,

    #[serde(default)]
    pub max_memory_mb: Option<u64>,

    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    #[serde(default)]
    pub allow_environment_variables: Vec<String>,
}

impl ToolCapabilities {
    #[must_use]
    pub fn no_access() -> Self {
        Self {
            network_access: false,
            filesystem_access: false,
            max_memory_mb: Some(128),
            timeout_seconds: Some(30),
            allow_environment_variables: vec![],
        }
    }
}

/// Complete descriptor for a tool including metadata and schemas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolDescriptor {
    pub schema_version: SchemaVersion,
    pub name: String,
    pub version: String,
    pub description: String,
    pub input_schema: JsonValue,
    pub bound_input_schema: JsonValue,
    pub output_schema: JsonValue,
    pub annotations: ToolAnnotations,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAnnotations {
    #[serde(default)]
    pub idempotent: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

impl ToolDescriptor {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.name.trim().is_empty() {
            return Err(ToolError::InvalidDescriptor("Tool name cannot be empty".to_string()));
        }

        if self.version.trim().is_empty() {
            return Err(ToolError::InvalidDescriptor("Tool version cannot be empty".to_string()));
        }

        if self.schema_version != SchemaVersion::V1 {
            return Err(ToolError::InvalidDescriptor("Unsupported schema version".to_string()));
        }

        Ok(())
    }
}

/// Core trait that all tool backends must implement
pub trait ToolBackend: Send + Sync {
    fn execute(&self, input: String, bound_input: String) -> Result<String, ToolError>;
    fn describe(&self) -> Result<ToolDescriptor, ToolError>;
}

pub type ToolResult<T> = Result<T, ToolError>;

pub mod backend;
pub mod policy;
pub mod registry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_validation() {
        let descriptor = ToolDescriptor {
            schema_version: SchemaVersion::V1,
            name: "test-tool".to_string(),
            version: "1.0.0".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({}),
            bound_input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            annotations: ToolAnnotations::default(),
        };

        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn test_empty_name_validation_failure() {
        let descriptor = ToolDescriptor {
            schema_version: SchemaVersion::V1,
            name: String::new(),
            version: "1.0.0".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({}),
            bound_input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            annotations: ToolAnnotations::default(),
        };

        assert!(matches!(descriptor.validate(), Err(ToolError::InvalidDescriptor(_))));
    }
}
