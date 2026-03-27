use thiserror::Error;

use crate::types::FfiOperation;

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("ffi operation not implemented: {operation}")]
    NotImplemented { operation: FfiOperation },

    #[error("failed to initialize async runtime: {message}")]
    RuntimeInitializationFailed { message: String },

    #[error("invalid ffi payload: {source}")]
    InvalidPayload {
        #[from]
        source: serde_json::Error,
    },
}
