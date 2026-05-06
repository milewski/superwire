use super::fixtures;
use crate::api::ExecutionOptions;
use crate::service::ExecutorService;
use crate::tests::support::{request_with_input, TestModelProvider, TrackingModelProvider};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_core::workflow_source;

#[tokio::test]
async fn dynamic_values_are_computed_and_used() {
    let output = execute!(
        fixtures::DYNAMIC_VALUES,
        input: { "topic": "rust async" },
        output: { "summary": "done" },
    )
    .await;

    assert_eq!(
        output,
        json!({
            "topic": "rust async",
            "audience": "engineering",
            "max_bullets": 3,
            "prompt_prefix": "Write a concise update",
            "summary": "done"
        })
    );
}

#[tokio::test]
async fn multiple_dynamic_blocks_are_merged() {
    let output = execute!(
        fixtures::DYNAMIC_VALUES,
        input: { "topic": "testing" },
        output: { "summary": "ok" },
    )
    .await;

    assert_eq!(output["audience"], "engineering");
    assert_eq!(output["max_bullets"], 3);
    assert_eq!(output["prompt_prefix"], "Write a concise update");
}

#[tokio::test]
async fn deterministic_tool_call_in_dynamic_block_executes_via_mcp() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        mcp local {
            endpoint: "__ENDPOINT__"
            headers: {
                Authorization: "Bearer test-token"
            }
        }

        input {
            project_id: number
            task_id: number
        }

        tool fetch_task_data from mcp.local.tool.fetch_task_data {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }
        }

        dynamic {
            data: call tool.fetch_task_data
        }

        agent summarizer {
            model: openai("model-a")
            prompt: "Summarize: {{ dynamic.data }}"
            output: {
                summary: string
            }
        }

        output {
            data: dynamic.data
            summary: agent.summarizer.summary
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(vec![json!({ "summary": "done" })]);
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "project_id": 42, "task_id": 7 })))
        .await
        .expect("deterministic tool call should execute successfully")
        .output;

    assert_eq!(output["data"], json!({ "task_title": "Survey", "participants": 10 }));
    assert_eq!(output["summary"], "done");
    assert_eq!(server.received_tool_arguments(), Some(json!({ "project_id": 42, "task_id": 7 })));
}

#[tokio::test]
async fn deterministic_tool_call_result_is_available_in_agent_prompt() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
            task_id: number
        }

        tool fetch_data from mcp.local.tool.fetch_task_data {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }
        }

        dynamic {
            result: call tool.fetch_data
        }

        agent processor {
            model: openai("model-a")
            prompt: "Process {{ dynamic.result }}"
            output: string
        }

        output {
            result: dynamic.result
            processed: agent.processor
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(vec![json!("processed")]);
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "project_id": 100, "task_id": 200 })))
        .await
        .expect("deterministic tool call result should be available in agent prompt")
        .output;

    assert_eq!(output["result"], json!({ "task_title": "Survey", "participants": 10 }));
    assert_eq!(output["processed"], "processed");
    assert_eq!(server.received_tool_arguments(), Some(json!({ "project_id": 100, "task_id": 200 })));
}

#[tokio::test]
async fn deterministic_tool_call_respects_max_calls_limit() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "__ENDPOINT__"
        }

        tool fetch_data from mcp.local.tool.fetch_task_data {
            max_calls: 1

            bindings {
                project_id: 1
                task_id: 2
            }
        }

        dynamic {
            first: call tool.fetch_data
            second: call tool.fetch_data
        }

        output {
            value: dynamic.second
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);

    let execution_error = service
        .execute(request_with_input(&workflow_source, Value::Null))
        .await
        .expect_err("execution should fail when deterministic tool call exceeds max_calls");

    assert!(execution_error.to_string().contains("cannot be called more than 1 times"));
}

#[tokio::test]
async fn agent_dynamic_tool_call_executes_inside_for_loop_agent() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        tool fetch_answer from mcp.local.tool.fetch_answer

        agent analyzer for task in [{ id: 1 }, { id: 2 }] {
            model: openai("model-a")

            dynamic {
                answer: call tool.fetch_answer {
                    input {
                        task_id: task.id
                    }
                }
            }

            prompt: "Context: ready"
            output: string
        }

        output {
            values: agent.analyzer
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let model_provider = TrackingModelProvider::new(vec![json!("first"), json!("second")]);
    let service = ExecutorService::new(model_provider.clone());
    let mut request = request_with_input(&workflow_source, Value::Null);

    request.options = ExecutionOptions {
        include_events: false,
        max_concurrency: 1,
    };

    let output = service
        .execute(request)
        .await
        .expect("agent dynamic tool call should execute inside for-loop agent")
        .output;

    assert_eq!(output["values"], json!(["first", "second"]));
    assert_eq!(
        model_provider.recorded_prompts(),
        vec!["Context: ready".to_string(), "Context: ready".to_string()]
    );
}

struct TestMcpHttpServer {
    endpoint: String,
    received_tool_arguments: Arc<Mutex<Option<Value>>>,
}

impl TestMcpHttpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let received_tool_arguments = Arc::new(Mutex::new(None));
        let thread_received_tool_arguments = Arc::clone(&received_tool_arguments);

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(10) {
                let stream = incoming_stream.expect("test MCP stream should open");
                handle_mcp_request(stream, &thread_received_tool_arguments);
            }
        });

        Self {
            endpoint,
            received_tool_arguments,
        }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn received_tool_arguments(&self) -> Option<Value> {
        self.received_tool_arguments
            .lock()
            .expect("received tool arguments lock should not be poisoned")
            .clone()
    }
}

fn handle_mcp_request(mut stream: TcpStream, received_tool_arguments: &Arc<Mutex<Option<Value>>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
    let mut content_length = 0_usize;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        reader.read_line(&mut header_line).expect("header line should read");

        if header_line == "\r\n" || header_line.is_empty() {
            break;
        }

        if let Some(value) = header_line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().expect("content length should parse");
        }
    }

    let mut request_body = vec![0_u8; content_length];
    reader.read_exact(&mut request_body).expect("request body should read");
    let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");

    if request.get("method").and_then(Value::as_str) == Some("tools/call") {
        *received_tool_arguments
            .lock()
            .expect("received tool arguments lock should not be poisoned") =
            request.get("params").and_then(|params| params.get("arguments")).cloned();
    }

    let response = if let Some(response_body) = response_for_request(&request) {
        let response_body = response_body.to_string();

        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        )
    } else {
        "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\n\r\n".to_string()
    };

    stream.write_all(response.as_bytes()).expect("response should write");
}

fn response_for_request(request: &Value) -> Option<Value> {
    match request.get("method").and_then(Value::as_str) {
        Some("notifications/initialized") => None,
        Some("tools/call") => Some(tool_call_response(request)),
        Some("tools/list") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "fetch_task_data",
                        "description": "Fetch task data",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "project_id": { "type": "number" },
                                "task_id": { "type": "number" }
                            },
                            "required": ["project_id", "task_id"]
                        },
                        "outputSchema": {
                            "type": "object",
                            "properties": {
                                "task_title": { "type": "string" },
                                "participants": { "type": "number" }
                            },
                            "required": ["task_title", "participants"]
                        }
                    },
                    {
                        "name": "fetch_answer",
                        "description": "Fetch answer",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "task_id": { "type": "number" }
                            },
                            "required": ["task_id"]
                        },
                        "outputSchema": {
                            "type": "object",
                            "properties": {
                                "answer": { "type": "string" }
                            },
                            "required": ["answer"]
                        }
                    }
                ]
            }
        })),
        _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}

fn tool_call_response(request: &Value) -> Value {
    let tool_name = request.get("params").and_then(|params| params.get("name")).and_then(Value::as_str);

    if tool_name == Some("fetch_answer") {
        let task_id = request
            .get("params")
            .and_then(|params| params.get("arguments"))
            .and_then(|arguments| arguments.get("task_id"))
            .and_then(Value::as_i64)
            .expect("fetch_answer task_id should be present");
        let answer = format!("answer for task {task_id}");

        return json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "content": [{ "type": "text", "text": format!("{{\"answer\":\"{answer}\"}}") }],
                "structuredContent": { "answer": answer }
            }
        });
    }

    json!({
        "jsonrpc": "2.0",
        "id": 2,
        "result": {
            "content": [{ "type": "text", "text": "{\"task_title\":\"Survey\",\"participants\":10}" }],
            "structuredContent": { "task_title": "Survey", "participants": 10 }
        }
    })
}
