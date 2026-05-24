use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use superwire_executor::runtime::ExecutorError;

pub struct ExecutorHttpError(pub ExecutorError);

impl From<ExecutorError> for ExecutorHttpError {
    fn from(error: ExecutorError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ExecutorHttpError {
    fn into_response(self) -> Response {
        let status = if self.0.is_client_error() {
            axum::http::StatusCode::BAD_REQUEST
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };

        let body = Json(json!({
            "error": self.0.to_string(),
        }));

        (status, body).into_response()
    }
}
