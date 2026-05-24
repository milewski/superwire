use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use superwire_semantic::WorkflowExecutionGraph;

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionRequest {
    #[serde(default)]
    pub workflow_source: Option<String>,

    #[serde(default)]
    pub workflow_source_base64: Option<String>,

    #[serde(default = "default_runtime_value")]
    pub input: Value,

    #[serde(default = "default_runtime_value")]
    pub secrets: Value,

    #[serde(default)]
    pub options: ExecutionOptions,
}

impl ExecutionRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.resolved_workflow_source().map(|_| ())
    }

    pub fn resolved_workflow_source(&self) -> Result<String, String> {
        match (&self.workflow_source, &self.workflow_source_base64) {
            (Some(workflow_source), None) => {
                if workflow_source.trim().is_empty() {
                    return Err("workflow_source must not be empty".to_string());
                }

                Ok(workflow_source.clone())
            }
            (None, Some(encoded_workflow_source)) => {
                if encoded_workflow_source.trim().is_empty() {
                    return Err("workflow_source_base64 must not be empty".to_string());
                }

                let decoded_source_bytes = BASE64_STANDARD
                    .decode(encoded_workflow_source)
                    .map_err(|error| format!("workflow_source_base64 must be valid standard base64: {error}"))?;
                let decoded_workflow_source = String::from_utf8(decoded_source_bytes)
                    .map_err(|error| format!("workflow_source_base64 must decode to valid UTF-8: {error}"))?;

                if decoded_workflow_source.trim().is_empty() {
                    return Err("decoded workflow_source_base64 must not be empty".to_string());
                }

                Ok(decoded_workflow_source)
            }
            (Some(_), Some(_)) => Err("send only one of workflow_source or workflow_source_base64".to_string()),
            (None, None) => Err("workflow_source or workflow_source_base64 is required".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionOptions {
    #[serde(default)]
    pub include_events: bool,

    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    #[serde(default = "default_use_cache")]
    pub use_cache: bool,

    #[serde(default)]
    pub cache_key: Option<String>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            include_events: false,
            max_concurrency: default_max_concurrency(),
            use_cache: default_use_cache(),
            cache_key: None,
        }
    }
}

impl ExecutionOptions {
    #[must_use]
    pub fn cache_key_identifier(&self) -> Option<&str> {
        self.cache_key.as_deref().map(str::trim).filter(|cache_key| !cache_key.is_empty())
    }
}

fn default_max_concurrency() -> usize {
    5
}

fn default_use_cache() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionResponse {
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheInvalidationResponse {
    pub purged_entries: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheInvalidationRequest {
    #[serde(default)]
    pub cache_key: Option<String>,
}

impl CacheInvalidationRequest {
    #[must_use]
    pub fn cache_key_identifier(&self) -> Option<&str> {
        self.cache_key.as_deref().map(str::trim).filter(|cache_key| !cache_key.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationRequest {
    #[serde(default)]
    pub workflow_source: Option<String>,

    #[serde(default)]
    pub workflow_source_base64: Option<String>,

    #[serde(default = "default_runtime_value")]
    pub secrets: Value,
}

impl ValidationRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.resolved_workflow_source().map(|_| ())
    }

    pub fn resolved_workflow_source(&self) -> Result<String, String> {
        match (&self.workflow_source, &self.workflow_source_base64) {
            (Some(workflow_source), None) => {
                if workflow_source.trim().is_empty() {
                    return Err("workflow_source must not be empty".to_string());
                }

                Ok(workflow_source.clone())
            }
            (None, Some(encoded_workflow_source)) => {
                if encoded_workflow_source.trim().is_empty() {
                    return Err("workflow_source_base64 must not be empty".to_string());
                }

                let decoded_source_bytes = BASE64_STANDARD
                    .decode(encoded_workflow_source)
                    .map_err(|error| format!("workflow_source_base64 must be valid standard base64: {error}"))?;
                let decoded_workflow_source = String::from_utf8(decoded_source_bytes)
                    .map_err(|error| format!("workflow_source_base64 must decode to valid UTF-8: {error}"))?;

                if decoded_workflow_source.trim().is_empty() {
                    return Err("decoded workflow_source_base64 must not be empty".to_string());
                }

                Ok(decoded_workflow_source)
            }
            (Some(_), Some(_)) => Err("send only one of workflow_source or workflow_source_base64".to_string()),
            (None, None) => Err("workflow_source or workflow_source_base64 is required".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationResponse {
    pub valid: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRequest {
    #[serde(default)]
    pub workflow_source: Option<String>,

    #[serde(default)]
    pub workflow_source_base64: Option<String>,

    #[serde(default = "default_runtime_value")]
    pub secrets: Value,
}

impl GraphRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.resolved_workflow_source().map(|_| ())
    }

    pub fn resolved_workflow_source(&self) -> Result<String, String> {
        match (&self.workflow_source, &self.workflow_source_base64) {
            (Some(workflow_source), None) => {
                if workflow_source.trim().is_empty() {
                    return Err("workflow_source must not be empty".to_string());
                }

                Ok(workflow_source.clone())
            }
            (None, Some(encoded_workflow_source)) => {
                if encoded_workflow_source.trim().is_empty() {
                    return Err("workflow_source_base64 must not be empty".to_string());
                }

                let decoded_source_bytes = BASE64_STANDARD
                    .decode(encoded_workflow_source)
                    .map_err(|error| format!("workflow_source_base64 must be valid standard base64: {error}"))?;
                let decoded_workflow_source = String::from_utf8(decoded_source_bytes)
                    .map_err(|error| format!("workflow_source_base64 must decode to valid UTF-8: {error}"))?;

                if decoded_workflow_source.trim().is_empty() {
                    return Err("decoded workflow_source_base64 must not be empty".to_string());
                }

                Ok(decoded_workflow_source)
            }
            (Some(_), Some(_)) => Err("send only one of workflow_source or workflow_source_base64".to_string()),
            (None, None) => Err("workflow_source or workflow_source_base64 is required".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphResponse {
    pub valid: bool,
    pub graph: WorkflowExecutionGraph,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormatRequest {
    #[serde(default)]
    pub workflow_source: Option<String>,

    #[serde(default)]
    pub workflow_source_base64: Option<String>,
}

impl FormatRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.resolved_workflow_source().map(|_| ())
    }

    pub fn resolved_workflow_source(&self) -> Result<String, String> {
        match (&self.workflow_source, &self.workflow_source_base64) {
            (Some(workflow_source), None) => {
                if workflow_source.trim().is_empty() {
                    return Err("workflow_source must not be empty".to_string());
                }

                Ok(workflow_source.clone())
            }
            (None, Some(encoded_workflow_source)) => {
                if encoded_workflow_source.trim().is_empty() {
                    return Err("workflow_source_base64 must not be empty".to_string());
                }

                let decoded_source_bytes = BASE64_STANDARD
                    .decode(encoded_workflow_source)
                    .map_err(|error| format!("workflow_source_base64 must be valid standard base64: {error}"))?;
                let decoded_workflow_source = String::from_utf8(decoded_source_bytes)
                    .map_err(|error| format!("workflow_source_base64 must decode to valid UTF-8: {error}"))?;

                if decoded_workflow_source.trim().is_empty() {
                    return Err("decoded workflow_source_base64 must not be empty".to_string());
                }

                Ok(decoded_workflow_source)
            }
            (Some(_), Some(_)) => Err("send only one of workflow_source or workflow_source_base64".to_string()),
            (None, None) => Err("workflow_source or workflow_source_base64 is required".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormatResponse {
    pub valid: bool,

    pub formatted_workflow_source: String,
}

fn default_runtime_value() -> Value {
    Value::Null
}
