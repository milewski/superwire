use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("ffi operation not implemented: {operation}")]
    NotImplemented { operation: String },

    #[error("invalid ffi payload: {source}")]
    InvalidPayload {
        #[from]
        source: serde_json::Error,
    },
}
