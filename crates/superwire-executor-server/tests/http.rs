mod support;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use superwire_executor::ExecutorService;
use superwire_executor_server::{executor_router_with_service, executor_router_with_service_and_playground_dist};
use superwire_mcp::{HttpMcpClientFactory, McpNetworkPolicy};
use superwire_protocol::{ExecutionOptions, ExecutionRequest, MAX_SERIALIZED_PUBLIC_EVENT_BYTES};
use support::fixtures;
use support::{ConcurrentTrackingModelProvider, TestMcpHttpServer, TrackingModelProvider};
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
    let request = ExecutionRequest {
        workflow_source: Some(fixtures::MINIMUM.to_string()),
        workflow_source_base64: Some(BASE64_STANDARD.encode(fixtures::MINIMUM)),
        input: json!(null),
        secrets: json!(null),
        options: ExecutionOptions::default(),
    };
    let error = service.execute(request).await.expect_err("ambiguous source should fail");
    assert!(error
        .to_string()
        .contains("send only one of workflow_source or workflow_source_base64"));
}

#[tokio::test]
async fn rejects_invalid_base64_source() {
    let service = support::service(vec![]);
    let request = ExecutionRequest {
        workflow_source: None,
        workflow_source_base64: Some("not base64!".to_string()),
        input: json!(null),
        secrets: json!(null),
        options: ExecutionOptions::default(),
    };
    let error = service.execute(request).await.expect_err("invalid base64 should fail");
    assert!(error.to_string().contains("workflow_source_base64 must be valid standard base64"));
}

#[tokio::test]
async fn base64_source_executes_successfully() {
    let output = support::execute(fixtures::MINIMUM, vec![json!({ "value": "ok" })]).await;

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
    let provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
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
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]), true);
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
async fn http_rejects_unknown_execution_options_with_typed_error() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let request_body = json!({
        "workflow_source": fixtures::MINIMUM,
        "options": {
            "include_events": true
        }
    });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "invalid_input");
    assert!(response_json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("unknown field `include_events`")));
}

#[tokio::test]
async fn http_rejects_zero_and_excessive_concurrency_before_provider_work() {
    let model_provider = TrackingModelProvider::new(Vec::new());
    let router = executor_router_with_service(ExecutorService::new(model_provider.clone()), true);

    for max_concurrency in [0, 65] {
        let request_body = json!({
            "workflow_source": fixtures::MINIMUM,
            "options": {
                "max_concurrency": max_concurrency
            }
        });
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/execute")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(request_body.to_string()))
            .expect("request should build");
        let response = router.clone().oneshot(request).await.expect("request should execute");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

        assert_eq!(response_json["error"]["code"], "invalid_input");
        assert!(response_json["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("must be between 1 and 64")));
    }

    assert_eq!(model_provider.recorded_count(), 0);
}

#[tokio::test]
async fn http_maps_bad_input_to_bad_request() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let request_body = json!({ "workflow_source": fixtures::INPUT_STRING, "input": { "topic": 123 } });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "invalid_input");
    assert_eq!(response_json["error"]["stage"], "input");
    assert_eq!(response_json["error"]["severity"], "error");
    assert_eq!(response_json["error"]["subject"]["type"], "workflow");
}

#[tokio::test]
async fn http_maps_model_panic_to_typed_internal_failure() {
    let router = executor_router_with_service(ExecutorService::new(support::PanickingModelProvider), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "internal_panic");
    assert_eq!(response_json["error"]["stage"], "internal");
    assert_eq!(response_json["error"]["message"], "workflow execution panicked");
    assert!(!String::from_utf8_lossy(&body).contains("scripted server model panic"));
}

#[tokio::test]
async fn http_maps_model_error_to_typed_bad_gateway_failure() {
    let router = executor_router_with_service(ExecutorService::new(support::FailingModelProvider::new("model overloaded")), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "model_provider_failed");
    assert_eq!(response_json["error"]["stage"], "model");
    assert_eq!(response_json["error"]["subject"]["type"], "provider");
}
#[tokio::test]
async fn http_accepts_base64_workflow_source() {
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]), true);
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

#[tokio::test]
async fn http_streams_events_when_accept_header_requests_event_stream() {
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or_default();

    assert!(content_type.starts_with("text/event-stream"));
    let run_identifier = response
        .headers()
        .get("x-superwire-run-id")
        .and_then(|header_value| header_value.to_str().ok())
        .expect("stream response should include run identifier")
        .to_string();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");

    let response_body = String::from_utf8(body.to_vec()).expect("response body should be valid UTF-8");
    assert!(response_body.contains("\"kind\":\"workflow_started\""));
    assert!(response_body.contains("\"kind\":\"workflow_completed\""));
    assert!(!response_body.contains(&run_identifier));
}

#[tokio::test]
async fn oversized_terminal_output_becomes_one_bounded_typed_failure_before_event_sequencing() {
    const SECRET_SENTINEL: &str = "superwire-secret-sentinel";

    let oversized_output = format!("{SECRET_SENTINEL}{}", "x".repeat(MAX_SERIALIZED_PUBLIC_EVENT_BYTES));
    let router = executor_router_with_service(support::service(vec![json!({ "value": oversized_output })]), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");
    let response = router.oneshot(request).await.expect("request should execute");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_body = String::from_utf8(body.to_vec()).expect("response body should be valid UTF-8");
    let mut event_identifiers = Vec::new();
    let mut terminal_events = Vec::new();

    for frame in response_body.split("\n\n").filter(|frame| frame.starts_with("event: ")) {
        assert!(frame.len() + 2 <= MAX_SERIALIZED_PUBLIC_EVENT_BYTES);

        let event_identifier = frame
            .lines()
            .find_map(|line| line.strip_prefix("id: "))
            .expect("event frame should include an identifier")
            .parse::<u64>()
            .expect("event identifier should be numeric");
        let event_data = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("event frame should include data");
        let event: serde_json::Value = serde_json::from_str(event_data).expect("event data should be JSON");

        event_identifiers.push(event_identifier);

        if matches!(
            event["kind"].as_str(),
            Some("workflow_completed" | "workflow_failed" | "workflow_cancelled")
        ) {
            terminal_events.push(event);
        }
    }

    assert_eq!(terminal_events.len(), 1);
    assert_eq!(terminal_events[0]["kind"], "workflow_failed");
    assert_eq!(terminal_events[0]["diagnostic"]["code"], "event_too_large");
    assert_eq!(terminal_events[0]["diagnostic"]["stage"], "output");
    assert_eq!(terminal_events[0]["diagnostic"]["subject"]["type"], "event");
    assert_eq!(
        terminal_events[0]["diagnostic"]["subject"]["maximum_bytes"],
        MAX_SERIALIZED_PUBLIC_EVENT_BYTES
    );
    assert!(terminal_events[0]["diagnostic"]["subject"]["actual_bytes"]
        .as_u64()
        .is_some_and(|actual_bytes| actual_bytes > MAX_SERIALIZED_PUBLIC_EVENT_BYTES as u64));
    assert!(terminal_events[0]["data"].get("output").is_none());
    assert!(!response_body.contains(SECRET_SENTINEL));
    assert_eq!(event_identifiers, (1..=event_identifiers.len() as u64).collect::<Vec<_>>());
}

#[tokio::test]
async fn http_returns_typed_unknown_run_outcomes() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let reconnect_request = axum::http::Request::builder()
        .method("GET")
        .uri("/execute/unknown-run/events")
        .header("accept", "text/event-stream")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let reconnect_response = router.clone().oneshot(reconnect_request).await.expect("request should execute");

    assert_eq!(reconnect_response.status(), axum::http::StatusCode::NOT_FOUND);

    let reconnect_body = axum::body::to_bytes(reconnect_response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let reconnect_json: serde_json::Value = serde_json::from_slice(&reconnect_body).expect("reconnect response should be JSON");

    assert_eq!(reconnect_json["error"]["code"], "unknown_run");
    assert_eq!(reconnect_json["error"]["subject"]["type"], "stream");
    assert!(reconnect_json["error"]["subject"].get("run_identifier").is_none());

    let cancel_request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute/unknown-run/cancel")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let cancel_response = router.oneshot(cancel_request).await.expect("request should execute");
    let cancel_body = axum::body::to_bytes(cancel_response.into_body(), usize::MAX)
        .await
        .expect("cancel response body should read");
    let cancel_json: serde_json::Value = serde_json::from_slice(&cancel_body).expect("cancel response should be JSON");

    assert_eq!(cancel_json, json!({ "transition": "unknown_run" }));
}

#[tokio::test]
async fn http_stream_reconnect_replays_events_after_last_event_identifier() {
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.clone().oneshot(request).await.expect("request should execute");
    let run_identifier = response
        .headers()
        .get("x-superwire-run-id")
        .and_then(|header_value| header_value.to_str().ok())
        .expect("stream response should include run identifier")
        .to_string();
    let initial_body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let initial_response_body = String::from_utf8(initial_body.to_vec()).expect("response body should be valid UTF-8");

    assert!(initial_response_body.contains("id: 1"));

    let reconnect_request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/execute/{run_identifier}/events"))
        .header("accept", "text/event-stream")
        .header("last-event-id", "1")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let reconnect_response = router.oneshot(reconnect_request).await.expect("request should execute");

    assert_eq!(reconnect_response.status(), axum::http::StatusCode::OK);

    let reconnect_body = axum::body::to_bytes(reconnect_response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let reconnect_response_body = String::from_utf8(reconnect_body.to_vec()).expect("response body should be valid UTF-8");

    assert!(!reconnect_response_body.contains("id: 1"));
    assert!(reconnect_response_body.contains("\"kind\":\"workflow_completed\""));
}

#[tokio::test]
async fn http_stream_reconnect_rejects_ahead_and_malformed_cursors() {
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");
    let response = router.clone().oneshot(request).await.expect("request should execute");
    let run_identifier = response
        .headers()
        .get("x-superwire-run-id")
        .and_then(|header_value| header_value.to_str().ok())
        .expect("stream response should include run identifier")
        .to_string();

    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("initial stream should complete");

    let ahead_request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/execute/{run_identifier}/events"))
        .header("accept", "text/event-stream")
        .header("last-event-id", "999")
        .body(axum::body::Body::empty())
        .expect("ahead request should build");
    let ahead_response = router.clone().oneshot(ahead_request).await.expect("ahead request should execute");

    assert_eq!(ahead_response.status(), axum::http::StatusCode::CONFLICT);

    let ahead_body = axum::body::to_bytes(ahead_response.into_body(), usize::MAX)
        .await
        .expect("ahead response body should read");
    let ahead_json: serde_json::Value = serde_json::from_slice(&ahead_body).expect("ahead response should be JSON");

    assert_eq!(ahead_json["error"]["code"], "stream_gap");
    assert!(ahead_json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("exceeds latest event identifier")));
    assert!(ahead_json["error"]["subject"].get("run_identifier").is_none());
    assert!(!serde_json::to_string(&ahead_json)
        .expect("ahead response should serialize")
        .contains(&run_identifier));

    let malformed_request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/execute/{run_identifier}/events"))
        .header("accept", "text/event-stream")
        .header("last-event-id", "not-an-identifier")
        .body(axum::body::Body::empty())
        .expect("malformed request should build");
    let malformed_response = router.oneshot(malformed_request).await.expect("malformed request should execute");

    assert_eq!(malformed_response.status(), axum::http::StatusCode::BAD_REQUEST);

    let malformed_body = axum::body::to_bytes(malformed_response.into_body(), usize::MAX)
        .await
        .expect("malformed response body should read");
    let malformed_json: serde_json::Value = serde_json::from_slice(&malformed_body).expect("malformed response should be JSON");

    assert_eq!(malformed_json["error"]["code"], "invalid_input");
    assert!(malformed_json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("invalid `last-event-id` header")));
}

#[tokio::test]
async fn http_stream_cancel_aborts_running_execution() {
    let model_provider = ConcurrentTrackingModelProvider::new(Duration::from_secs(30));
    let router = executor_router_with_service(ExecutorService::new(model_provider.clone()), true);
    let request_body = json!({ "workflow_source": fixtures::MINIMUM });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/execute")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.clone().oneshot(request).await.expect("request should execute");
    let run_identifier = response
        .headers()
        .get("x-superwire-run-id")
        .and_then(|header_value| header_value.to_str().ok())
        .expect("stream response should include run identifier")
        .to_string();

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.active_requests() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("workflow should start model work before cancellation");

    let cancel_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/execute/{run_identifier}/cancel"))
        .body(axum::body::Body::empty())
        .expect("request should build");
    let cancel_response = router.clone().oneshot(cancel_request).await.expect("request should execute");

    assert_eq!(cancel_response.status(), axum::http::StatusCode::OK);

    let cancel_body = axum::body::to_bytes(cancel_response.into_body(), usize::MAX)
        .await
        .expect("cancel response body should read");
    let cancel_json: serde_json::Value = serde_json::from_slice(&cancel_body).expect("cancel response should be JSON");

    assert_eq!(cancel_json, json!({ "transition": "accepted" }));

    let repeated_cancel_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/execute/{run_identifier}/cancel"))
        .body(axum::body::Body::empty())
        .expect("request should build");
    let repeated_cancel_response = router
        .clone()
        .oneshot(repeated_cancel_request)
        .await
        .expect("request should execute");
    let repeated_cancel_body = axum::body::to_bytes(repeated_cancel_response.into_body(), usize::MAX)
        .await
        .expect("repeated cancel response body should read");
    let repeated_cancel_json: serde_json::Value =
        serde_json::from_slice(&repeated_cancel_body).expect("repeated cancel response should be JSON");

    assert!(matches!(
        repeated_cancel_json["transition"].as_str(),
        Some("already_requested" | "already_terminal")
    ));

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.active_requests() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled workflow should abort active model work");

    let reconnect_request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/execute/{run_identifier}/events"))
        .header("accept", "text/event-stream")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let reconnect_response = router.clone().oneshot(reconnect_request).await.expect("request should execute");

    assert_eq!(reconnect_response.status(), axum::http::StatusCode::OK);

    let reconnect_body = axum::body::to_bytes(reconnect_response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let reconnect_response_body = String::from_utf8(reconnect_body.to_vec()).expect("response body should be valid UTF-8");

    assert!(reconnect_response_body.contains("\"kind\":\"workflow_cancelled\""));
    assert!(reconnect_response_body.contains("\"code\":\"cancelled\""));

    let terminal_cancel_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/execute/{run_identifier}/cancel"))
        .body(axum::body::Body::empty())
        .expect("request should build");
    let terminal_cancel_response = router.oneshot(terminal_cancel_request).await.expect("request should execute");
    let terminal_cancel_body = axum::body::to_bytes(terminal_cancel_response.into_body(), usize::MAX)
        .await
        .expect("terminal cancel response body should read");
    let terminal_cancel_json: serde_json::Value =
        serde_json::from_slice(&terminal_cancel_body).expect("terminal cancel response should be JSON");

    assert_eq!(terminal_cancel_json, json!({ "transition": "already_terminal" }));
}

#[tokio::test]
async fn http_validate_returns_success_without_execution() {
    let router = executor_router_with_service(support::service(vec![json!("unused")]), true);
    let request_body = json!({
        "workflow_source": fixtures::INPUT_STRING
    });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/validate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");
    assert_eq!(response_json, json!({ "valid": true }));
}

#[tokio::test]
async fn http_validate_rejects_input_field() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let request_body = json!({
        "workflow_source": fixtures::INPUT_STRING,
        "input": { "topic": 123 }
    });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/validate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "invalid_input");
    assert_eq!(response_json["error"]["stage"], "input");
    assert!(response_json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("unknown field `input`")));
}

#[tokio::test]
async fn default_http_routes_reject_local_mcp_before_any_request() {
    let server = TestMcpHttpServer::spawn(Vec::<(String, String)>::new());
    let router = executor_router_with_service(support::service(vec![]), true);
    let workflow_source = superwire_macros::workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        secrets {
            mcp_endpoint: string
            mcp_token: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
            headers {
                Authorization: secrets.mcp_token
            }
        }

        tool local_update_user from mcp.local.tool.update_user_name

        agent updater {
            model: model.openai_model
            uses: [tool.local_update_user]
            instruction: "Rename the user"
            output {
                value: string
            }
        }

        output {
            value: agent.updater.value
        }
    };

    for route in ["/execute", "/validate", "/graph"] {
        let request_body = json!({
            "workflow_source": workflow_source,
            "secrets": {
                "mcp_endpoint": server.endpoint(),
                "mcp_token": "Bearer secret-token"
            }
        });
        let request = axum::http::Request::builder()
            .method("POST")
            .uri(route)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(request_body.to_string()))
            .expect("request should build");
        let response = router.clone().oneshot(request).await.expect("request should execute");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

        assert_eq!(response_json["error"]["code"], "invalid_configuration");
        assert_eq!(response_json["error"]["stage"], "mcp");
        assert!(response_json["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("outbound MCP networking is disabled")));
    }

    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn http_validate_with_secrets_resolves_mcp_schemas_without_input() {
    let server = TestMcpHttpServer::spawn([("authorization".to_string(), "Bearer secret-token".to_string())]);
    let service =
        support::service(vec![]).with_mcp_client_factory(Arc::new(HttpMcpClientFactory::for_network_policy(McpNetworkPolicy::Trusted)));
    let router = executor_router_with_service(service, true);
    let workflow_source = superwire_macros::workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        secrets {
            mcp_endpoint: string
            mcp_token: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
            headers {
                Authorization: secrets.mcp_token
            }
        }

        tool local_update_user from mcp.local.tool.update_user_name

        agent updater {
            model: model.openai_model
            uses: [tool.local_update_user]
            instruction: "Rename the user"
            output {
                value: string
            }
        }

        output {
            value: agent.updater.value
        }
    };

    let request_body = json!({
        "workflow_source": workflow_source,
        "secrets": {
            "mcp_endpoint": server.endpoint(),
            "mcp_token": "Bearer secret-token"
        }
    });

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/validate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");

    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json, json!({ "valid": true }));
    assert!(server.request_count() > 0);
}

#[tokio::test]
async fn http_graph_returns_agent_relationships_and_tools() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let workflow_source = superwire_macros::workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        tool lookup {
            input {
                topic: string
            }

            output {
                result: string
            }
        }

        agent first {
            model: model.openai_model
            uses: [tool.lookup]
            instruction: "Research"
            output {
                value: string
            }
        }

        agent second {
            model: model.openai_model
            instruction: agent.first.value
            output {
                value: string
            }
        }

        output {
            value: agent.second.value
        }
    };
    let request_body = json!({ "workflow_source": workflow_source });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/graph")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["valid"], json!(true));
    assert_eq!(response_json["graph"]["agent_execution_order"], json!(["first", "second"]));
    assert!(response_json["graph"]["edges"]
        .as_array()
        .expect("edges should be an array")
        .iter()
        .any(|edge| edge["source"] == json!("first") && edge["target"] == json!("second")));
    assert!(response_json["graph"]["nodes"]
        .as_array()
        .expect("nodes should be an array")
        .iter()
        .any(|node| node["id"] == json!("first") && node["tools"][0]["name"] == json!("lookup")));
}

#[tokio::test]
async fn http_format_formats_source_after_validation() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let request_body = json!({
        "workflow_source": "output { greeting: \"ok\" }"
    });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/format")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");
    assert_eq!(response_json["valid"], json!(true));
    assert!(response_json["formatted_workflow_source"].as_str().is_some());
}

#[tokio::test]
async fn playground_serves_root_redirect_and_index() {
    let playground_dist_directory = create_playground_dist_fixture();
    let router = executor_router_with_service_and_playground_dist(support::service(vec![]), false, playground_dist_directory);

    let root_request = axum::http::Request::builder()
        .uri("/")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let root_response = router.clone().oneshot(root_request).await.expect("request should execute");

    assert_eq!(root_response.status(), axum::http::StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        root_response
            .headers()
            .get("location")
            .and_then(|header_value| header_value.to_str().ok()),
        Some("/playground")
    );

    let playground_request = axum::http::Request::builder()
        .uri("/playground")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let playground_response = router.oneshot(playground_request).await.expect("request should execute");

    assert_eq!(playground_response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn playground_serves_logo_and_built_assets() {
    let playground_dist_directory = create_playground_dist_fixture();
    let router = executor_router_with_service_and_playground_dist(support::service(vec![]), false, playground_dist_directory);

    let logo_request = axum::http::Request::builder()
        .uri("/playground/logo-horizontal.svg")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let logo_response = router.clone().oneshot(logo_request).await.expect("request should execute");

    assert_eq!(logo_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        logo_response
            .headers()
            .get("content-type")
            .and_then(|header_value| header_value.to_str().ok()),
        Some("image/svg+xml")
    );

    let asset_request = axum::http::Request::builder()
        .uri("/playground/assets/playground.js")
        .body(axum::body::Body::empty())
        .expect("request should build");
    let asset_response = router.oneshot(asset_request).await.expect("request should execute");

    assert_eq!(asset_response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn http_cache_invalidation_rejects_empty_key_with_typed_error() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/cache/invalidate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json!({ "cache_key": " " }).to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let response_json: serde_json::Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json["error"]["code"], "invalid_input");
    assert_eq!(response_json["error"]["stage"], "input");
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

fn create_playground_dist_fixture() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    let playground_dist_directory = std::env::temp_dir().join(format!("superwire-playground-dist-test-{}-{timestamp}", std::process::id()));
    let asset_directory = playground_dist_directory.join("assets");

    std::fs::create_dir_all(&asset_directory).expect("playground fixture asset directory should be created");
    std::fs::write(playground_dist_directory.join("index.html"), "<main>Playground</main>")
        .expect("playground fixture index should be written");
    std::fs::write(
        playground_dist_directory.join("logo-horizontal.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"></svg>",
    )
    .expect("playground fixture logo should be written");
    std::fs::write(asset_directory.join("playground.js"), "console.log('playground');")
        .expect("playground fixture asset should be written");

    playground_dist_directory
}
