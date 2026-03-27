use crate::error::FfiError;
use crate::types::{FfiRequestEnvelope, FfiResponseEnvelope};

#[derive(Debug, Default)]
pub struct EngineFfi;

impl EngineFfi {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    pub fn invoke(&self, request_envelope: FfiRequestEnvelope) -> Result<FfiResponseEnvelope, FfiError> {
        let operation = request_envelope.operation();

        Err(FfiError::NotImplemented { operation })
    }
}
