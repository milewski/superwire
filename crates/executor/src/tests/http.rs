use super::fixtures;
use super::support;
use super::support::TrackingModelProvider;
use crate::server::executor_router_with_service;
use crate::service::ExecutorService;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde_json::json;
use tower::util::ServiceExt;

#[tokio::test]
async fn rejects_empty_workflow_source() {
    let service = support::service(vec![]);
    let request = support::request("  ");
    let error = service.execute(request).await.expect_err("empty workflow should fail");
    assert!(error.to_string().contains("workflow_source must not be empty"));
}

#[tokio::test]
async fn rejects_request_with_both_source_encodings() {
    let service = support::service(vec![]);
    let request = crate::ExecutionRequest {
        workflow_source: Some(fixtures::MINIMUM.to_string()),
        workflow_source_base64: Some(BASE64_STANDARD.encode(fixtures::MINIMUM)),
        input: json!(null),
        secrets: json!(null),
        options: crate::ExecutionOptions::default(),
    };
    let error = service.execute(request).await.expect_err("ambiguous source should fail");
    assert!(error
        .to_string()
        .contains("send only one of workflow_source or workflow_source_base64"));
}

#[tokio::test]
async fn rejects_invalid_base64_source() {
    let service = support::service(vec![]);
    let request = crate::ExecutionRequest {
        workflow_source: None,
        workflow_source_base64: Some("not base64!".to_string()),
        input: json!(null),
        secrets: json!(null),
        options: crate::ExecutionOptions::default(),
    };
    let error = service.execute(request).await.expect_err("invalid base64 should fail");
    assert!(error.to_string().contains("workflow_source_base64 must be valid standard base64"));
}

#[tokio::test]
async fn base64_source_executes_successfully() {
    let output = execute!(fixtures::MINIMUM, output: "ok").await;
    assert_eq!(output, json!({ "greeting": "ok" }));
}

#[tokio::test]
async fn model_provider_error_propagates() {
    let service = ExecutorService::new(support::FailingModelProvider::new("model overloaded"));
    let request = support::request(fixtures::MINIMUM);
    let error = service.execute(request).await.expect_err("model failure should propagate");
    assert!(error.to_string().contains("model overloaded"));
}

#[tokio::test]
async fn tracking_provider_records_all_calls() {
    let provider = TrackingModelProvider::new(vec![json!("first"), json!("second")]);
    let service = ExecutorService::new(provider.clone());
    let request = support::request_with_input(fixtures::LINEAR_CHAIN, json!({ "topic": "testing" }));

    service.execute(request).await.expect("execution should succeed");

    assert_eq!(provider.recorded_count(), 2);
    let agent_names = provider.recorded_agent_names();
    assert!(agent_names.contains(&"first".to_string()));
    assert!(agent_names.contains(&"second".to_string()));
}

#[tokio::test]
async fn http_returns_final_output() {
    let router = executor_router_with_service(support::service(vec![json!("ok")]));
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");
    assert_eq!(response_json, json!({ "output": { "greeting": "ok" } }));
}

#[tokio::test]
async fn http_maps_bad_input_to_bad_request() {
    let router = executor_router_with_service(support::service(vec![]));
    let request_body = json!({ "workflow_source": fixtures::INPUT_STRING, "input": { "topic": 123 } });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_accepts_base64_workflow_source() {
    let router = executor_router_with_service(support::service(vec![json!("ok")]));
    let request_body = json!({ "workflow_source_base64": BASE64_STANDARD.encode(fixtures::MINIMUM) });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");
    assert_eq!(response_json, json!({ "output": { "greeting": "ok" } }));
}
