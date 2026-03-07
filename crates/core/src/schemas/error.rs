use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("schema compilation failed: {message}")]
    Compile { message: String },
    #[error("schema validation failed: {messages}")]
    Validation { messages: String },
    #[error("schema references are not supported in this compiler step: `{name}`")]
    UnsupportedReference { name: String },
    #[error("schema implementation is not available yet")]
    Unimplemented,
}
