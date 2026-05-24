use super::fixtures;
use super::support::{request, TrackingModelProvider};
use crate::runtime::AgentCacheSession;
use crate::server::executor_router_with_service;
use crate::service::ExecutorService;
use serde_json::json;
use tower::util::ServiceExt;

#[tokio::test]
async fn service_reuses_cached_agent_output_for_same_session() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let first_response = service
        .execute_for_session(request(fixtures::MINIMUM), cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::MINIMUM), cache_session)
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "first" }));
    assert_eq!(model_provider.recorded_count(), 1);
}

#[tokio::test]
async fn service_reuses_cached_agent_output_for_same_client_cache_key() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let mut first_request = request(fixtures::MINIMUM);
    let mut second_request = request(fixtures::MINIMUM);
    first_request.options.cache_key = Some("client-cache-key".to_string());
    second_request.options.cache_key = Some("client-cache-key".to_string());
    let first_response = service.execute(first_request).await.expect("first execution should succeed");
    let second_response = service.execute(second_request).await.expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "first" }));
    assert_eq!(model_provider.recorded_count(), 1);
}

#[tokio::test]
async fn service_skips_cache_without_client_cache_key() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let first_response = service
        .execute(request(fixtures::MINIMUM))
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute(request(fixtures::MINIMUM))
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn service_skips_cache_when_request_disables_it() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let mut first_request = request(fixtures::MINIMUM);
    let mut second_request = request(fixtures::MINIMUM);
    first_request.options.use_cache = false;
    second_request.options.use_cache = false;
    let first_response = service
        .execute_for_session(first_request, cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(second_request, cache_session)
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn service_separates_cache_by_session() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let first_response = service
        .execute_for_session(request(fixtures::MINIMUM), AgentCacheSession::new("browser-a"))
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::MINIMUM), AgentCacheSession::new("browser-b"))
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn http_cache_invalidation_purges_session_entries() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let router = executor_router_with_service(service, true);
    let request_body = json!({
        "workflow_source": fixtures::MINIMUM,
        "options": { "cache_key": "browser-a" }
    });
    let first_request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");
    let first_response = router.clone().oneshot(first_request).await.expect("request should execute");

    assert_eq!(first_response.status(), axum::http::StatusCode::OK);

    let purge_request = axum::http::Request::builder()
        .method("POST")
        .uri("/cache/invalidate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json!({ "cache_key": "browser-a" }).to_string()))
        .expect("request should build");
    let purge_response = router.clone().oneshot(purge_request).await.expect("request should execute");

    assert_eq!(purge_response.status(), axum::http::StatusCode::OK);

    let second_request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");
    let second_response = router.oneshot(second_request).await.expect("request should execute");

    assert_eq!(second_response.status(), axum::http::StatusCode::OK);
    assert_eq!(model_provider.recorded_count(), 2);
}
