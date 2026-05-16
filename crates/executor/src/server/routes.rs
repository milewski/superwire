use crate::api::{ExecutionRequest, FormatRequest, ValidationRequest};
use crate::model::{ModelProvider, OpenAiModelProvider};
use crate::server::error::ExecutorHttpError;
use crate::server::sse::event_to_sse_result;
use crate::service::ExecutorService;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::Sse;
use axum::response::Redirect;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use superwire_lsp::server::LanguageServer;
use tokio::fs;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;

pub fn executor_router() -> Router {
    executor_router_with_service(ExecutorService::new(OpenAiModelProvider), false)
}

pub fn executor_router_with_service<ModelProviderType>(service: ExecutorService<ModelProviderType>, disable_playground: bool) -> Router
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let router = Router::new()
        .route("/execute", post(execute_handler::<ModelProviderType>))
        .route("/validate", post(validate_handler::<ModelProviderType>))
        .route("/format", post(format_handler::<ModelProviderType>))
        .route("/lsp", axum::routing::get(lsp_websocket_handler));

    let router = if disable_playground {
        router
    } else {
        router
            .route("/", axum::routing::get(playground_root_redirect_handler))
            .route("/playground", axum::routing::get(playground_index_handler))
            .route("/playground/{*path}", axum::routing::get(playground_asset_or_index_handler))
    };

    router.with_state(service)
}

pub async fn serve_executor(address: SocketAddr, disable_playground: bool) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;

    axum::serve(
        listener,
        executor_router_with_service(ExecutorService::new(OpenAiModelProvider), disable_playground),
    )
    .await
}

async fn execute_handler<ModelProviderType>(
    State(service): State<ExecutorService<ModelProviderType>>,
    request_headers: HeaderMap,
    Json(request): Json<ExecutionRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    match ExecuteResponseKind::from_headers(&request_headers) {
        ExecuteResponseKind::Json => {
            let response = service.execute(request).await?;

            Ok(Json(response).into_response())
        }

        ExecuteResponseKind::EventStream => {
            let event_receiver = service.execute_stream(request);
            let event_stream = ReceiverStream::new(event_receiver).map(event_to_sse_result);

            Ok(Sse::new(event_stream).into_response())
        }
    }
}

enum ExecuteResponseKind {
    Json,
    EventStream,
}

impl ExecuteResponseKind {
    fn from_headers(request_headers: &HeaderMap) -> Self {
        let Some(accept_header) = request_headers.get(header::ACCEPT) else {
            return Self::Json;
        };

        let Ok(accept_value) = accept_header.to_str() else {
            return Self::Json;
        };

        for media_type in accept_value.split(',') {
            let media_type = media_type.trim();
            let normalized_media_type = media_type.split(';').next().unwrap_or(media_type).trim();

            if normalized_media_type.eq_ignore_ascii_case("text/event-stream") {
                return Self::EventStream;
            }
        }

        Self::Json
    }
}

async fn validate_handler<ModelProviderType>(
    State(service): State<ExecutorService<ModelProviderType>>,
    Json(request): Json<ValidationRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Ok(Json(service.validate(request)?).into_response())
}

async fn format_handler<ModelProviderType>(
    State(service): State<ExecutorService<ModelProviderType>>,
    Json(request): Json<FormatRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Ok(Json(service.format(request)?).into_response())
}

async fn lsp_websocket_handler(websocket_upgrade: WebSocketUpgrade) -> Response {
    websocket_upgrade.on_upgrade(handle_lsp_websocket)
}

async fn playground_root_redirect_handler() -> Redirect {
    Redirect::temporary("/playground")
}

async fn playground_index_handler() -> Response {
    serve_index_file().await
}

async fn playground_asset_or_index_handler(Path(requested_path): Path<String>) -> Response {
    let normalized_requested_path = requested_path.trim_start_matches('/');
    let normalized_requested_path = normalized_requested_path
        .strip_prefix("playground/")
        .unwrap_or(normalized_requested_path);

    if normalized_requested_path.contains("..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let static_file_path = playground_dist_directory().join(normalized_requested_path);

    if let Ok(file_metadata) = fs::metadata(&static_file_path).await {
        if file_metadata.is_file() {
            return serve_file_response(static_file_path).await;
        }
    }

    if normalized_requested_path.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }

    serve_index_file().await
}

async fn handle_lsp_websocket(websocket: WebSocket) {
    let (mut websocket_sender, mut websocket_receiver) = websocket.split();
    let mut language_server = LanguageServer::default();

    while let Some(message_result) = websocket_receiver.next().await {
        let Ok(message) = message_result else {
            break;
        };

        let raw_message = match message {
            Message::Text(text) => text.to_string().into_bytes(),
            Message::Binary(bytes) => bytes.to_vec(),
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => continue,
        };

        let server_messages = match language_server.handle_json_rpc_message(&raw_message) {
            Ok(server_messages) => server_messages,
            Err(error) => {
                log::warn!("failed to handle websocket LSP message: {error}");

                continue;
            }
        };

        for server_message in server_messages.messages {
            let response_text = server_message.to_string();

            if websocket_sender.send(Message::Text(response_text.into())).await.is_err() {
                return;
            }
        }

        if server_messages.should_exit {
            break;
        }
    }
}

async fn serve_index_file() -> Response {
    serve_file_response(playground_dist_directory().join("index.html")).await
}

async fn serve_file_response(file_path: PathBuf) -> Response {
    let Ok(file_bytes) = fs::read(&file_path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_type = content_type_for_path(&file_path);

    let mut response = Response::new(file_bytes.into_response().into_body());
    response.headers_mut().insert(header::CONTENT_TYPE, content_type);

    response
}

fn content_type_for_path(file_path: &std::path::Path) -> HeaderValue {
    let extension = file_path.extension().and_then(|extension| extension.to_str()).unwrap_or("");

    let mime_type = match extension {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    };

    HeaderValue::from_static(mime_type)
}

fn playground_dist_directory() -> PathBuf {
    if let Ok(playground_dist_directory) = std::env::var("SUPERWIRE_PLAYGROUND_DIST") {
        return PathBuf::from(playground_dist_directory);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../playground/dist")
}
