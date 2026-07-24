#[macro_use]
mod support;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use superwire_protocol::event::{ExecutorDiagnosticCode, ExecutorEventKind};
use support::fixtures;
use support::runner::TestRunner;

const SENSITIVE_MCP_DETAIL: &str = "private-upstream-tool-error-detail";

#[tokio::test]
async fn model_recovers_from_mcp_success_envelope_with_error_result() {
    let mcp_server = ErrorResultMcpServer::spawn();
    let output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .respond_tool_calls([call!("create_sorting_task", { "title": "first" })]);

                model
                    .turn()
                    .with_messages(assert_model_received_typed_tool_error)
                    .respond_json(json!({ "value": "recovered after MCP tool failure" }));
            });
        })
        .mcp_http_endpoint("local", mcp_server.endpoint())
        .run()
        .await
        .expect("model should recover from an MCP isError tool result");

    assert_eq!(output.output, json!({ "value": { "value": "recovered after MCP tool failure" } }));
    assert_eq!(mcp_server.method_count("tools/call"), 1);

    let started_events = output
        .events
        .iter()
        .filter(|event| event.kind == ExecutorEventKind::McpCallStarted)
        .collect::<Vec<_>>();
    let failed_events = output
        .events
        .iter()
        .filter(|event| event.kind == ExecutorEventKind::McpCallFailed)
        .collect::<Vec<_>>();
    let completed_event_count = output
        .events
        .iter()
        .filter(|event| event.kind == ExecutorEventKind::McpCallCompleted)
        .count();

    assert_eq!(started_events.len(), 1);
    assert_eq!(failed_events.len(), 1);
    assert_eq!(completed_event_count, 0);
    assert_matching_lifecycle(started_events[0], failed_events[0]);

    let failed_diagnostic = failed_events[0]
        .diagnostic
        .as_ref()
        .expect("failed MCP call should include a diagnostic");

    assert_eq!(failed_diagnostic.code, ExecutorDiagnosticCode::McpFailed);
    assert_eq!(failed_diagnostic.message, "MCP call failed for `create_sorting_task`");

    let public_event_history = serde_json::to_string(&output.events).expect("event history should serialize");
    let provider_requests = serde_json::to_string(&output.provider_requests).expect("provider requests should serialize");

    assert!(!public_event_history.contains(SENSITIVE_MCP_DETAIL));
    assert!(!provider_requests.contains(SENSITIVE_MCP_DETAIL));
}

fn assert_model_received_typed_tool_error(messages: &[Value]) {
    let tool_message = messages
        .iter()
        .find(|message| message.get("role") == Some(&json!("tool")))
        .expect("MCP error should be returned to the model as a tool result");
    let tool_content = tool_message
        .get("content")
        .and_then(Value::as_str)
        .expect("tool result content should be text");
    let tool_error: Value = serde_json::from_str(tool_content).expect("tool error should be typed JSON");

    assert_eq!(tool_error.get("error"), Some(&json!("mcp_tool_call_failed")));
    assert_eq!(tool_error.get("server_name"), Some(&json!("local")));
    assert_eq!(tool_error.get("tool_name"), Some(&json!("create-sorting-task")));
    assert_eq!(tool_error.get("message"), Some(&json!("MCP tool reported a failure")));
    assert!(!tool_content.contains(SENSITIVE_MCP_DETAIL));
}

fn assert_matching_lifecycle(
    started_event: &superwire_protocol::event::ExecutorEvent,
    failed_event: &superwire_protocol::event::ExecutorEvent,
) {
    let started_data = started_event.data.as_ref().expect("started event should include details");
    let failed_data = failed_event.data.as_ref().expect("failed event should include details");

    for field_name in ["operation", "target_name", "server_name", "item_name", "argument_names"] {
        assert_eq!(started_data.get(field_name), failed_data.get(field_name));
    }

    assert_eq!(failed_data.get("operation"), Some(&json!("call")));
    assert_eq!(failed_data.get("target_name"), Some(&json!("create_sorting_task")));
    assert_eq!(failed_data.get("server_name"), Some(&json!("local")));
    assert_eq!(failed_data.get("item_name"), Some(&json!("create-sorting-task")));
    assert!(failed_data.get("error").is_none());
    assert_eq!(
        failed_event
            .diagnostic
            .as_ref()
            .expect("failed event should include a diagnostic")
            .code,
        superwire_protocol::event::ExecutorDiagnosticCode::McpFailed
    );
}

struct ErrorResultMcpServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    shutdown: Arc<AtomicBool>,
    server_thread: Option<JoinHandle<()>>,
}

impl ErrorResultMcpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("MCP listener should bind");
        listener.set_nonblocking(true).expect("MCP listener should become nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().expect("MCP listener address should exist"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let server_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _peer_address)) => {
                        let request = read_json_request(&stream).expect("MCP request should parse");
                        let response = response_for_request(&request);

                        thread_requests
                            .lock()
                            .expect("MCP request lock should not be poisoned")
                            .push(request);
                        stream.write_all(response.as_bytes()).expect("MCP response should write");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("MCP listener failed: {error}"),
                }
            }
        });

        Self {
            endpoint,
            requests,
            shutdown,
            server_thread: Some(server_thread),
        }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn method_count(&self, method: &str) -> usize {
        self.requests
            .lock()
            .expect("MCP request lock should not be poisoned")
            .iter()
            .filter(|request| request.get("method").and_then(Value::as_str) == Some(method))
            .count()
    }
}

impl Drop for ErrorResultMcpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);

        if let Some(server_thread) = self.server_thread.take() {
            server_thread.join().expect("MCP server thread should stop");
        }
    }
}

fn response_for_request(request: &Value) -> String {
    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();

    if method == "notifications/initialized" {
        return "HTTP/1.1 202 Accepted\r\nconnection: close\r\ncontent-length: 0\r\n\r\n".to_string();
    }

    let result = match method {
        "initialize" => json!({}),
        "tools/list" => tools_list_result(),
        "resources/list" => json!({ "resources": [] }),
        "resources/templates/list" => json!({ "resourceTemplates": [] }),
        "prompts/list" => json!({ "prompts": [] }),
        "tools/call" => json!({
            "content": [{ "type": "text", "text": SENSITIVE_MCP_DETAIL }],
            "isError": true,
        }),
        _ => panic!("unexpected MCP method `{method}`"),
    };
    let response_body = json!({
        "jsonrpc": "2.0",
        "id": request.get("id").cloned().unwrap_or_else(|| json!(1)),
        "result": result,
    });

    http_json_response(response_body)
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {
                "name": "create-sorting-task",
                "description": "Create a sorting task",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project_id": { "type": "number" },
                        "task_id": { "type": "number" },
                        "title": { "type": "string" },
                    },
                    "required": ["project_id", "task_id", "title"],
                },
                "outputSchema": {
                    "type": "object",
                    "properties": { "task_id": { "type": "number" } },
                    "required": ["task_id"],
                },
            },
            {
                "name": "update-task-status",
                "description": "Update task status",
                "inputSchema": { "type": "object" },
                "outputSchema": { "type": "object" },
            },
            {
                "name": "assign-task",
                "description": "Assign task",
                "inputSchema": { "type": "object" },
                "outputSchema": { "type": "object" },
            },
        ],
    })
}

fn read_json_request(stream: &TcpStream) -> Option<Value> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;

    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut content_length = 0_usize;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        reader.read_line(&mut header_line).ok()?;

        if header_line == "\r\n" || header_line.is_empty() {
            break;
        }

        if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
            if header_name.eq_ignore_ascii_case("content-length") {
                content_length = header_value.trim().parse().ok()?;
            }
        }
    }

    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).ok()?;

    serde_json::from_slice(&body).ok()
}

fn http_json_response(body: Value) -> String {
    let body_text = body.to_string();

    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
        body_text.len(),
        body_text
    )
}
