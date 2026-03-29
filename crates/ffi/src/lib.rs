pub mod bridge;
pub mod error;
#[cfg(feature = "php-ext")]
pub mod php_extension;
pub mod types;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde::Serialize;
use serde_json::json;

pub use bridge::{CustomToolHandler, EngineFfi};
pub use error::FfiError;
pub use types::{
    CustomToolDeclaration, ExecutionValueName, FfiOperation, FfiRequest, FfiRequestEnvelope, FfiResponse, FfiResponseEnvelope,
    ReadExecutionValueEnvelope, ReadExecutionValueRequest, ReadExecutionValueSuccess, ToolCallbackConfig, ToolInvocationEnvelope,
    ToolInvocationError, ToolInvocationErrorCode, ToolInvocationPayload, ToolInvocationResult, WorkflowExecutionEnvelope,
    WorkflowExecutionError, WorkflowExecutionErrorCode, WorkflowExecutionInput, WorkflowExecutionOutput, WorkflowExecutionRequest,
    WorkflowExecutionSecrets, FFI_PROTOCOL_VERSION,
};

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum FfiBoundaryEnvelope {
    Succeeded { response: FfiResponseEnvelope },
    Failed { error: FfiBoundaryError },
}

impl FfiBoundaryEnvelope {
    #[must_use]
    fn from_request_payload(request_payload: &str) -> Self {
        let request_envelope = match serde_json::from_str::<FfiRequestEnvelope>(request_payload) {
            Ok(request_envelope) => request_envelope,
            Err(error) => {
                return Self::Failed {
                    error: FfiBoundaryError::new(FfiBoundaryErrorCode::InvalidRequestPayload, error.to_string()),
                };
            }
        };

        let operation = request_envelope.operation();
        let request_id = request_envelope.request_id.clone();
        let ffi_engine = EngineFfi::new();
        let response_envelope = match ffi_engine.invoke(request_envelope) {
            Ok(response_envelope) => response_envelope,
            Err(error) => FfiResponseEnvelope::from_operation_error(operation, request_id, &error),
        };

        Self::Succeeded {
            response: response_envelope,
        }
    }

    #[must_use]
    fn into_json_payload(self) -> String {
        serde_json::to_string(&self)
            .unwrap_or_else(|error| Self::failed_json_payload(FfiBoundaryErrorCode::SerializationFailed, error.to_string()))
    }

    fn into_c_string_pointer_with_fallback(self) -> *mut c_char {
        json_payload_into_c_string_pointer_with_fallback(self.into_json_payload())
    }

    fn failed_json_payload(error_code: FfiBoundaryErrorCode, message: String) -> String {
        json!({
            "status": FfiBoundaryStatus::Failed,
            "error": {
                "code": error_code,
                "message": message,
            },
        })
        .to_string()
    }
}

#[must_use]
pub fn invoke_ffi_json_payload(request_payload: &str) -> String {
    FfiBoundaryEnvelope::from_request_payload(request_payload).into_json_payload()
}

fn json_payload_into_c_string_pointer_with_fallback(json_payload: String) -> *mut c_char {
    match CString::new(json_payload) {
        Ok(owned_c_string) => owned_c_string.into_raw(),
        Err(_) => CString::new(FfiBoundaryEnvelope::failed_json_payload(
            FfiBoundaryErrorCode::SerializationFailed,
            String::from("ffi response contains an interior NUL byte"),
        ))
        .map_or(std::ptr::null_mut(), CString::into_raw),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FfiBoundaryStatus {
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct FfiBoundaryError {
    code: FfiBoundaryErrorCode,
    message: String,
}

impl FfiBoundaryError {
    #[must_use]
    fn new(code: FfiBoundaryErrorCode, message: String) -> Self {
        Self { code, message }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FfiBoundaryErrorCode {
    NullRequestPointer,
    InvalidRequestEncoding,
    InvalidRequestPayload,
    SerializationFailed,
}

/// Invokes the engine FFI bridge using a JSON request envelope.
///
/// # Safety
///
/// `request_json_pointer` must either be null or point to a valid, NUL-terminated UTF-8 C string
/// that remains alive for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_ffi_invoke_json(request_json_pointer: *const c_char) -> *mut c_char {
    if request_json_pointer.is_null() {
        let boundary_envelope = FfiBoundaryEnvelope::Failed {
            error: FfiBoundaryError::new(
                FfiBoundaryErrorCode::NullRequestPointer,
                String::from("request pointer must not be null"),
            ),
        };

        return boundary_envelope.into_c_string_pointer_with_fallback();
    }

    let request_c_string = { unsafe { CStr::from_ptr(request_json_pointer) } };

    let request_payload = match request_c_string.to_str() {
        Ok(request_payload) => request_payload,
        Err(error) => {
            let boundary_envelope = FfiBoundaryEnvelope::Failed {
                error: FfiBoundaryError::new(FfiBoundaryErrorCode::InvalidRequestEncoding, error.to_string()),
            };

            return boundary_envelope.into_c_string_pointer_with_fallback();
        }
    };

    let response_payload = invoke_ffi_json_payload(request_payload);

    json_payload_into_c_string_pointer_with_fallback(response_payload)
}

/// Frees a JSON response pointer returned by `engine_ffi_invoke_json`.
///
/// # Safety
///
/// `owned_json_pointer` must be either null or a pointer previously returned by
/// `engine_ffi_invoke_json` that has not yet been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_ffi_free_json(owned_json_pointer: *mut c_char) {
    if owned_json_pointer.is_null() {
        return;
    }

    unsafe {
        let _owned_c_string = CString::from_raw(owned_json_pointer);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{FfiBoundaryEnvelope, FfiBoundaryErrorCode};

    #[test]
    fn builds_failed_payload_with_enum_serialization() {
        let failed_payload = FfiBoundaryEnvelope::failed_json_payload(
            FfiBoundaryErrorCode::SerializationFailed,
            String::from("failed to serialize ffi response"),
        );

        let parsed_payload =
            serde_json::from_str::<serde_json::Value>(&failed_payload).expect("failed payload should always be valid json");

        assert_eq!(
            parsed_payload,
            json!({
                "status": "failed",
                "error": {
                    "code": "serialization_failed",
                    "message": "failed to serialize ffi response",
                },
            })
        );
    }
}
