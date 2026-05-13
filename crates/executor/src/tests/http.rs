use super::fixtures;
use super::support;
use super::support::TrackingModelProvider;
use super::tools::TestMcpHttpServer;
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
    let output = execute!(fixtures::MINIMUM, output: { "value": "ok" }).await;
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
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]));
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
    let router = executor_router_with_service(support::service(vec![json!({ "value": "ok" })]));
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
async fn http_validate_returns_success_without_execution() {
    let router = executor_router_with_service(support::service(vec![json!("unused")]));
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
    let router = executor_router_with_service(support::service(vec![]));
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
    assert_eq!(response.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body_text = String::from_utf8(body.to_vec()).expect("response body should be UTF-8");
    assert!(body_text.contains("unknown field `input`"));
}

#[tokio::test]
async fn http_validate_with_secrets_resolves_mcp_schemas_without_input() {
    let server = TestMcpHttpServer::spawn([("authorization".to_string(), "Bearer secret-token".to_string())]);
    let router = executor_router_with_service(support::service(vec![]));
    let workflow_source = superwire_core::workflow_source! {
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
            headers: {
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
}

#[tokio::test]
async fn http_format_formats_source_after_validation() {
    let router = executor_router_with_service(support::service(vec![]));
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
