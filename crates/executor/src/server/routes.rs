use crate::api::{ExecutionRequest, FormatRequest, GraphRequest, ValidationRequest};
use crate::model::{CerseiModelProvider, ModelProvider};
use crate::runtime::{AgentCacheConfig, AgentCacheDriver, AgentCacheSession};
use crate::server::error::ExecutorHttpError;
use crate::server::sse::event_to_sse_result;
use crate::service::ExecutorService;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{KeepAlive, Sse};
use axum::response::Redirect;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use superwire_lsp::server::LanguageServer;
use tokio::fs;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;

const RUN_IDENTIFIER_HEADER: &str = "x-superwire-run-id";
const CACHE_SESSION_HEADER: &str = "x-superwire-session-id";

#[derive(Clone)]
struct ExecutorRouterState<ModelProviderType> {
    service: ExecutorService<ModelProviderType>,
    playground_dist_directory: PathBuf,
}

pub fn executor_router() -> Router {
    executor_router_with_service(ExecutorService::new(CerseiModelProvider), false)
}

pub fn executor_router_with_service<ModelProviderType>(service: ExecutorService<ModelProviderType>, disable_playground: bool) -> Router
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    executor_router_with_service_and_playground_dist(service, disable_playground, default_playground_dist_directory())
}

pub(crate) fn executor_router_with_service_and_playground_dist<ModelProviderType>(
    service: ExecutorService<ModelProviderType>,
    disable_playground: bool,
    playground_dist_directory: PathBuf,
) -> Router
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let state = ExecutorRouterState {
        service,
        playground_dist_directory,
    };

    let router = Router::new()
        .route("/execute", post(execute_handler::<ModelProviderType>))
        .route(
            "/execute/{run_identifier}/events",
            axum::routing::get(reconnect_stream_handler::<ModelProviderType>),
        )
        .route("/cache/invalidate", post(invalidate_cache_handler::<ModelProviderType>))
        .route("/validate", post(validate_handler::<ModelProviderType>))
        .route("/graph", post(graph_handler::<ModelProviderType>))
        .route("/format", post(format_handler::<ModelProviderType>))
        .route("/lsp", axum::routing::get(lsp_websocket_handler));

    let router = if disable_playground {
        router
    } else {
        router
            .route("/", axum::routing::get(playground_root_redirect_handler))
            .route("/playground", axum::routing::get(playground_index_handler::<ModelProviderType>))
            .route(
                "/playground/{*path}",
                axum::routing::get(playground_asset_or_index_handler::<ModelProviderType>),
            )
    };

    router.with_state(state)
}

pub async fn serve_executor(address: SocketAddr, disable_playground: bool) -> Result<(), std::io::Error> {
    serve_executor_with_cache(
        address,
        disable_playground,
        AgentCacheDriver::InMemory,
        crate::runtime::DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
    )
    .await
}

pub async fn serve_executor_with_cache(
    address: SocketAddr,
    disable_playground: bool,
    cache_driver: AgentCacheDriver,
    cache_time_to_live: Duration,
) -> Result<(), std::io::Error> {
    serve_executor_with_agent_cache(address, disable_playground, AgentCacheConfig::new(cache_driver), cache_time_to_live).await
}

pub async fn serve_executor_with_agent_cache(
    address: SocketAddr,
    disable_playground: bool,
    cache_config: AgentCacheConfig,
    cache_time_to_live: Duration,
) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;
    let service =
        ExecutorService::with_agent_cache_config(CerseiModelProvider, cache_config, cache_time_to_live).map_err(std::io::Error::other)?;

    axum::serve(listener, executor_router_with_service(service, disable_playground)).await
}

async fn execute_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    request_headers: HeaderMap,
    Json(request): Json<ExecutionRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    match ExecuteResponseKind::from_headers(&request_headers) {
        ExecuteResponseKind::Json => {
            let cache_session = cache_session_from_headers(&request_headers);
            let response = state.service.execute_for_session(request, cache_session).await?;

            Ok(Json(response).into_response())
        }

        ExecuteResponseKind::EventStream => {
            let cache_session = cache_session_from_headers(&request_headers);
            let stream_subscription = state.service.start_streamed_execution_for_session(request, cache_session);
            let run_identifier = stream_subscription.run_identifier.clone();
            let event_stream = ReceiverStream::new(stream_subscription.receiver).map(event_to_sse_result);

            Ok(sse_response(event_stream, &run_identifier))
        }
    }
}

async fn invalidate_cache_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    request_headers: HeaderMap,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let cache_session = cache_session_from_headers(&request_headers);
    let response = state.service.invalidate_agent_cache_session(&cache_session)?;

    Ok(Json(response).into_response())
}

async fn reconnect_stream_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    Path(run_identifier): Path<String>,
    Query(reconnect_parameters): Query<StreamReconnectParameters>,
    request_headers: HeaderMap,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let last_event_identifier = reconnect_parameters.last_event_identifier(&request_headers);
    let Some(stream_subscription) = state.service.reconnect_streamed_execution(&run_identifier, last_event_identifier) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    let event_stream = ReceiverStream::new(stream_subscription.receiver).map(event_to_sse_result);

    Ok(sse_response(event_stream, &run_identifier))
}

#[derive(Debug, Deserialize)]
struct StreamReconnectParameters {
    #[serde(default)]
    after: Option<u64>,
}

impl StreamReconnectParameters {
    fn last_event_identifier(&self, request_headers: &HeaderMap) -> Option<u64> {
        self.after.or_else(|| {
            request_headers
                .get("last-event-id")
                .and_then(|header_value| header_value.to_str().ok())
                .and_then(|header_value| header_value.parse::<u64>().ok())
        })
    }
}

fn sse_response<StreamType>(event_stream: StreamType, run_identifier: &str) -> Response
where
    StreamType: futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>> + Send + 'static,
{
    let keep_alive = KeepAlive::new().interval(Duration::from_secs(10)).text("workflow stream active");
    let mut response = Sse::new(event_stream).keep_alive(keep_alive).into_response();

    if let Ok(header_value) = HeaderValue::from_str(run_identifier) {
        response.headers_mut().insert(RUN_IDENTIFIER_HEADER, header_value);
    }

    response
}

fn cache_session_from_headers(request_headers: &HeaderMap) -> AgentCacheSession {
    if let Some(session_identifier) = request_headers
        .get(CACHE_SESSION_HEADER)
        .and_then(|header_value| header_value.to_str().ok())
        .filter(|header_value| !header_value.trim().is_empty())
    {
        return AgentCacheSession::new(session_identifier);
    }

    let user_agent = request_headers
        .get(header::USER_AGENT)
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or_default();
    let accept_language = request_headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or_default();
    let forwarded_for = request_headers
        .get("x-forwarded-for")
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or_default();
    let real_ip = request_headers
        .get("x-real-ip")
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or_default();

    AgentCacheSession::from_fingerprint_parts(&[user_agent, accept_language, forwarded_for, real_ip])
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
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    Json(request): Json<ValidationRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Ok(Json(state.service.validate(request)?).into_response())
}

async fn graph_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    Json(request): Json<GraphRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Ok(Json(state.service.graph(request)?).into_response())
}

async fn format_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    Json(request): Json<FormatRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Ok(Json(state.service.format(request)?).into_response())
}

async fn lsp_websocket_handler(websocket_upgrade: WebSocketUpgrade) -> Response {
    websocket_upgrade.on_upgrade(handle_lsp_websocket)
}

async fn playground_root_redirect_handler() -> Redirect {
    Redirect::temporary("/playground")
}

async fn playground_index_handler<ModelProviderType>(State(state): State<ExecutorRouterState<ModelProviderType>>) -> Response {
    serve_index_file(&state.playground_dist_directory).await
}

async fn playground_asset_or_index_handler<ModelProviderType>(
    State(state): State<ExecutorRouterState<ModelProviderType>>,
    Path(requested_path): Path<String>,
) -> Response {
    let normalized_requested_path = requested_path.trim_start_matches('/');
    let normalized_requested_path = normalized_requested_path
        .strip_prefix("playground/")
        .unwrap_or(normalized_requested_path);

    if normalized_requested_path.contains("..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let static_file_path = state.playground_dist_directory.join(normalized_requested_path);

    if let Ok(file_metadata) = fs::metadata(&static_file_path).await {
        if file_metadata.is_file() {
            return serve_file_response(static_file_path).await;
        }
    }

    if normalized_requested_path.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }

    serve_index_file(&state.playground_dist_directory).await
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

async fn serve_index_file(playground_dist_directory: &std::path::Path) -> Response {
    serve_file_response(playground_dist_directory.join("index.html")).await
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

fn default_playground_dist_directory() -> PathBuf {
    if let Ok(playground_dist_directory) = std::env::var("SUPERWIRE_PLAYGROUND_DIST") {
        return PathBuf::from(playground_dist_directory);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../playground/dist")
}
