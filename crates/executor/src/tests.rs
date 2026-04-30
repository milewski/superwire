use crate::api::{ExecutionOptions, ExecutionRequest};
use crate::event::ExecutorEventKind;
use crate::model::{ModelProvider, ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use crate::server::executor_router_with_service;
use crate::service::ExecutorService;
use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Debug, Clone)]
struct TestModelProvider {
    outputs: Arc<Mutex<VecDeque<Value>>>,
}

impl TestModelProvider {
    fn new(outputs: Vec<Value>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(VecDeque::from(outputs))),
        }
    }
}

#[async_trait]
impl ModelProvider for TestModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let output = self
            .outputs
            .lock()
            .expect("test runner outputs lock should not be poisoned")
            .pop_front()
            .unwrap_or_else(|| json!(request.agent_name));

        Ok(ModelResponse {
            output,
            context: json!({ "agent": request.agent_name }),
        })
    }
}

fn linear_workflow_source() -> String {
    superwire_core::workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        input {
            topic: string
        }

        agent first {
            model: openai("model-a")
            prompt: input.topic
            output: string
        }

        agent second {
            model: openai("model-a")
            prompt: agent.first
            output: string
        }

        output {
            result: agent.second
        }
    }
    .to_string()
}

#[tokio::test]
async fn executes_workflow_request_with_dynamic_json_input() {
    let service = ExecutorService::new(TestModelProvider::new(vec![json!("first output"), json!("final output")]));
    let request = ExecutionRequest {
        workflow_source: Some(linear_workflow_source()),
        workflow_source_base64: None,
        input: json!({ "topic": "testing" }),
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };

    let response = service.execute(request).await.expect("execution should succeed");

    assert_eq!(response.output, json!({ "result": "final output" }));
}

#[tokio::test]
async fn streamed_execution_emits_lifecycle_events() {
    let service = ExecutorService::new(TestModelProvider::new(vec![json!("first output"), json!("final output")]));
    let request = ExecutionRequest {
        workflow_source: Some(linear_workflow_source()),
        workflow_source_base64: None,
        input: json!({ "topic": "testing" }),
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };
    let mut event_receiver = service.execute_stream(request);
    let mut event_kinds = Vec::new();

    while let Some(event) = event_receiver.recv().await {
        event_kinds.push(event.kind);
    }

    assert_eq!(event_kinds.first(), Some(&ExecutorEventKind::WorkflowStarted));
    assert!(event_kinds.contains(&ExecutorEventKind::WorkflowPlanned));
    assert!(event_kinds.contains(&ExecutorEventKind::AgentStarted));
    assert!(event_kinds.contains(&ExecutorEventKind::AgentCompleted));
    assert_eq!(event_kinds.last(), Some(&ExecutorEventKind::WorkflowCompleted));
}

#[tokio::test]
async fn rejects_empty_workflow_source() {
    let service = ExecutorService::new(TestModelProvider::new(Vec::new()));
    let request = ExecutionRequest {
        workflow_source: Some("  ".to_string()),
        workflow_source_base64: None,
        input: Value::Null,
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };

    let error = service.execute(request).await.expect_err("empty workflow should fail");

    assert!(error.to_string().contains("workflow_source must not be empty"));
}

#[tokio::test]
async fn executes_workflow_request_with_base64_source() {
    let service = ExecutorService::new(TestModelProvider::new(vec![json!("first output"), json!("final output")]));
    let encoded_workflow_source = BASE64_STANDARD.encode(linear_workflow_source());
    let request = ExecutionRequest {
        workflow_source: None,
        workflow_source_base64: Some(encoded_workflow_source),
        input: json!({ "topic": "testing" }),
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };

    let response = service.execute(request).await.expect("execution should succeed");

    assert_eq!(response.output, json!({ "result": "final output" }));
}

#[tokio::test]
async fn rejects_request_with_both_source_encodings() {
    let service = ExecutorService::new(TestModelProvider::new(Vec::new()));
    let request = ExecutionRequest {
        workflow_source: Some(linear_workflow_source()),
        workflow_source_base64: Some(BASE64_STANDARD.encode(linear_workflow_source())),
        input: json!({ "topic": "testing" }),
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };

    let error = service.execute(request).await.expect_err("ambiguous workflow source should fail");

    assert!(error
        .to_string()
        .contains("send only one of workflow_source or workflow_source_base64"));
}

#[tokio::test]
async fn rejects_invalid_base64_source() {
    let service = ExecutorService::new(TestModelProvider::new(Vec::new()));
    let request = ExecutionRequest {
        workflow_source: None,
        workflow_source_base64: Some("not base64!".to_string()),
        input: Value::Null,
        secrets: Value::Null,
        options: ExecutionOptions::default(),
    };

    let error = service.execute(request).await.expect_err("invalid base64 source should fail");

    assert!(error.to_string().contains("workflow_source_base64 must be valid standard base64"));
}

#[tokio::test]
async fn http_execute_returns_final_output() {
    let router = executor_router_with_service(ExecutorService::new(TestModelProvider::new(vec![json!("ok")])));
    let request_body = json!({
        "workflow_source": superwire_core::workflow_source! {
            provider openai {
                driver: "openai"
                endpoint: "http://localhost:1234/v1"
                api_key: "test-api-key"
                models: ["model-a"]
            }

            agent assistant {
                model: openai("model-a")
                prompt: "hello"
                output: string
            }

            output {
                result: agent.assistant
            }
        },
    });
    let request = Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.expect("response body should read");
    let response_json: Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json, json!({ "output": { "result": "ok" } }));
}

#[tokio::test]
async fn http_execute_accepts_base64_workflow_source() {
    let router = executor_router_with_service(ExecutorService::new(TestModelProvider::new(vec![json!("ok")])));
    let workflow_source = superwire_core::workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        agent assistant {
            model: openai("model-a")
            prompt: "hello"
            output: string
        }

        output {
            result: agent.assistant
        }
    };
    let request_body = json!({
        "workflow_source_base64": BASE64_STANDARD.encode(workflow_source),
    });
    let request = Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.expect("response body should read");
    let response_json: Value = serde_json::from_slice(&body).expect("response should be JSON");

    assert_eq!(response_json, json!({ "output": { "result": "ok" } }));
}

#[tokio::test]
async fn http_execute_maps_bad_input_to_bad_request() {
    let router = executor_router_with_service(ExecutorService::new(TestModelProvider::new(Vec::new())));
    let request_body = json!({
        "workflow_source": superwire_core::workflow_source! {
            input { topic: string }
            output { result: input.topic }
        },
        "input": { "topic": 123 }
    });
    let request = Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("request should build");

    let response = router.oneshot(request).await.expect("request should execute");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
