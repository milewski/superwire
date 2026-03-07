use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider `{driver}` is not registered")]
    UnknownDriver { driver: String },
    #[error("provider request failed: {message}")]
    RequestFailed { message: String },
    #[error("provider implementation is not available yet")]
    Unimplemented,
}
