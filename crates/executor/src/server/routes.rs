use crate::api::ExecutionRequest;
use crate::model::{ModelProvider, OpenAiModelProvider};
use crate::server::error::ExecutorHttpError;
use crate::server::sse::event_to_sse_result;
use crate::service::ExecutorService;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;

pub fn executor_router() -> Router {
    executor_router_with_service(ExecutorService::new(OpenAiModelProvider))
}

pub fn executor_router_with_service<ModelProviderType>(service: ExecutorService<ModelProviderType>) -> Router
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/execute", post(execute_handler::<ModelProviderType>))
        .route("/execute/stream", post(execute_stream_handler::<ModelProviderType>))
        .with_state(service)
}

pub async fn serve_executor(address: SocketAddr) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;

    axum::serve(listener, executor_router()).await
}

async fn execute_handler<ModelProviderType>(
    State(service): State<ExecutorService<ModelProviderType>>,
    Json(request): Json<ExecutionRequest>,
) -> Result<Response, ExecutorHttpError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let response = service.execute(request).await?;

    Ok(Json(response).into_response())
}

async fn execute_stream_handler<ModelProviderType>(
    State(service): State<ExecutorService<ModelProviderType>>,
    Json(request): Json<ExecutionRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let event_receiver = service.execute_stream(request);
    let event_stream = ReceiverStream::new(event_receiver).map(event_to_sse_result);

    Sse::new(event_stream)
}
