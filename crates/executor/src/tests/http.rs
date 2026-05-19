use super::fixtures;
use super::support;
use super::support::TrackingModelProvider;
use super::tools::TestMcpHttpServer;
use crate::server::{executor_router_with_service, executor_router_with_service_and_playground_dist};
use crate::service::ExecutorService;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde_json::json;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");

    let response_body = String::from_utf8(body.to_vec()).expect("response body should be valid UTF-8");
    assert!(response_body.contains("\"kind\":\"workflow_started\""));
    assert!(response_body.contains("\"kind\":\"workflow_completed\""));
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
    let router = executor_router_with_service(support::service(vec![]), true);
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
}

#[tokio::test]
async fn http_graph_returns_agent_relationships_and_tools() {
    let router = executor_router_with_service(support::service(vec![]), true);
    let workflow_source = superwire_core::workflow_source! {
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
