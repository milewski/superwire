use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("ffi workflow execution is not implemented yet")]
    NotImplemented,
}

pub fn execute_workflow_placeholder(_input: Value) -> Result<Value, FfiError> {
    Err(FfiError::NotImplemented)
}

#[cfg(feature = "php-ext")]
pub fn php_extension_enabled() -> bool {
    true
}
