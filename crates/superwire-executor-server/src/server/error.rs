use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use superwire_executor::runtime::ExecutorError;
use superwire_protocol::event::ExecutorDiagnosticCode;

pub struct ExecutorHttpError(pub ExecutorError);

impl ExecutorHttpError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self(ExecutorError::invalid_input(message))
    }

    fn status(is_client_error: bool, diagnostic_code: ExecutorDiagnosticCode) -> StatusCode {
        if is_client_error {
            return StatusCode::BAD_REQUEST;
        }

        match diagnostic_code {
            ExecutorDiagnosticCode::UnknownRun => StatusCode::NOT_FOUND,
            ExecutorDiagnosticCode::StreamExpired => StatusCode::GONE,
            ExecutorDiagnosticCode::StreamGap | ExecutorDiagnosticCode::CancellationConflict => StatusCode::CONFLICT,
            ExecutorDiagnosticCode::ModelProviderFailed
            | ExecutorDiagnosticCode::ProviderRateLimited
            | ExecutorDiagnosticCode::ProviderRetriesExhausted => StatusCode::BAD_GATEWAY,
            ExecutorDiagnosticCode::CacheUnavailable | ExecutorDiagnosticCode::StreamCapacityExceeded => StatusCode::SERVICE_UNAVAILABLE,
            ExecutorDiagnosticCode::InvalidWorkflow
            | ExecutorDiagnosticCode::InvalidInput
            | ExecutorDiagnosticCode::InvalidSecrets
            | ExecutorDiagnosticCode::InvalidConfiguration => StatusCode::BAD_REQUEST,
            ExecutorDiagnosticCode::EventTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ExecutorDiagnosticCode::InvalidOutput
            | ExecutorDiagnosticCode::ModelRejected
            | ExecutorDiagnosticCode::ToolFailed
            | ExecutorDiagnosticCode::McpFailed
            | ExecutorDiagnosticCode::Cancelled
            | ExecutorDiagnosticCode::InternalPanic
            | ExecutorDiagnosticCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<ExecutorError> for ExecutorHttpError {
    fn from(error: ExecutorError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ExecutorHttpError {
    fn into_response(self) -> Response {
        let is_client_error = self.0.is_client_error();
        let diagnostic = self.0.diagnostic();
        let status = Self::status(is_client_error, diagnostic.code);
        let body = Json(json!({
            "error": diagnostic,
        }));

        (status, body).into_response()
    }
}
