use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            include_events: false,
            max_concurrency: default_max_concurrency(),
        }
    }
}

fn default_max_concurrency() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionResponse {
    pub output: Value,
}

fn default_runtime_value() -> Value {
    Value::Null
}
