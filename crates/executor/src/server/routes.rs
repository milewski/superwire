use crate::api::{ExecutionRequest, FormatRequest, ValidationRequest};
use crate::model::{ModelProvider, OpenAiModelProvider};
use crate::server::error::ExecutorHttpError;
use crate::server::sse::event_to_sse_result;
use crate::service::ExecutorService;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::sse::Sse;
use axum::response::{IntoResponse, Response};
use axum::routing::{get_service, post};
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use superwire_lsp::server::LanguageServer;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::services::{ServeDir, ServeFile};

pub fn executor_router() -> Router {
    executor_router_with_service(ExecutorService::new(OpenAiModelProvider))
}

pub fn executor_router_with_service<ModelProviderType>(service: ExecutorService<ModelProviderType>) -> Router
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/execute", post(execute_handler::<ModelProviderType>))
        .route("/validate", post(validate_handler::<ModelProviderType>))
        .route("/format", post(format_handler::<ModelProviderType>))
        .route("/lsp", axum::routing::get(lsp_websocket_handler))
        .nest_service("/playground", get_service(playground_static_service()))
        .with_state(service)
}

pub async fn serve_executor(address: SocketAddr) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;

    axum::serve(listener, executor_router()).await
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

fn playground_static_service() -> ServeDir<ServeFile> {
    let playground_dist_directory = playground_dist_directory();
    let playground_index_path = playground_dist_directory.join("index.html");

    ServeDir::new(playground_dist_directory).fallback(ServeFile::new(playground_index_path))
}

fn playground_dist_directory() -> PathBuf {
    if let Ok(playground_dist_directory) = std::env::var("SUPERWIRE_PLAYGROUND_DIST") {
        return PathBuf::from(playground_dist_directory);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../playground/dist")
}
