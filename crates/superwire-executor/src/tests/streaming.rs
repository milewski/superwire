use super::fixtures;
use super::support;
use crate::event::ExecutorEventKind;
use crate::service::ExecutorService;
use crate::tests::support::{request_with_input, ConcurrentTrackingModelProvider, TestModelProvider, TrackingModelProvider};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;
use superwire_macros::workflow_source;

#[tokio::test]
async fn lifecycle_events_are_emitted_in_order() {
    let service = support::service(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let request = support::request_with_input(fixtures::LINEAR_CHAIN, json!({ "topic": "testing" }));
    let mut receiver = service.execute_stream(request);
    let mut kinds = Vec::new();

    while let Some(event) = receiver.recv().await {
        kinds.push(event.kind);
    }

    assert_eq!(kinds.first(), Some(&ExecutorEventKind::WorkflowStarted));
    assert!(kinds.contains(&ExecutorEventKind::WorkflowPlanned));
    assert!(kinds.contains(&ExecutorEventKind::AgentStarted));
    assert!(kinds.contains(&ExecutorEventKind::AgentCompleted));
    assert_eq!(kinds.last(), Some(&ExecutorEventKind::WorkflowCompleted));
}

#[tokio::test]
async fn agent_names_are_included_in_events() {
    let service = support::service(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let request = support::request_with_input(fixtures::LINEAR_CHAIN, json!({ "topic": "testing" }));
    let mut receiver = service.execute_stream(request);
    let mut agent_names = Vec::new();

    while let Some(event) = receiver.recv().await {
        if let Some(name) = event.agent_name {
            agent_names.push(name);
        }
    }

    assert!(agent_names.contains(&"first".to_string()));
    assert!(agent_names.contains(&"second".to_string()));
}

#[tokio::test]
async fn events_include_internal_tool_names_and_timings() {
    let service = support::service(vec![json!({ "value": "done" })]);
    let request = support::request_with_input(fixtures::INPUT_STRING, json!({ "topic": "testing" }));
    let mut receiver = service.execute_stream(request);
    let mut agent_started_tools = Vec::new();
    let mut saw_timestamp = false;
    let mut saw_duration = false;

    while let Some(event) = receiver.recv().await {
        saw_timestamp = saw_timestamp || event.timestamp_ms > 0;

        if event.kind == ExecutorEventKind::AgentStarted {
            agent_started_tools = event.data.as_ref().unwrap()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool_name| tool_name.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
        }

        if matches!(event.kind, ExecutorEventKind::AgentCompleted | ExecutorEventKind::WorkflowCompleted) {
            saw_duration = event.data.as_ref().unwrap()["duration_ms"].as_u64().is_some();
        }
    }

    assert!(saw_timestamp, "expected events to include timestamp_ms");
    assert!(saw_duration, "expected completion events to include duration_ms");
    assert!(agent_started_tools.contains(&"internal:finalize".to_string()));
    assert!(!agent_started_tools.contains(&"finalize".to_string()));
}

#[tokio::test]
async fn failure_emits_workflow_failed_event() {
    let service = support::service(vec![]);
    let request = support::request_with_input(fixtures::INPUT_STRING, json!({ "topic": 123 }));
    let mut receiver = service.execute_stream(request);
    let mut kinds = Vec::new();

    while let Some(event) = receiver.recv().await {
        kinds.push(event.kind);
    }

    assert_eq!(kinds.first(), Some(&ExecutorEventKind::WorkflowStarted));
    assert_eq!(kinds.last(), Some(&ExecutorEventKind::WorkflowFailed));
}

#[tokio::test]
async fn dropping_stream_receiver_does_not_cancel_running_workflow() {
    let model_provider = ConcurrentTrackingModelProvider::new(Duration::from_millis(300));
    let service = ExecutorService::new(model_provider.clone());
    let request = support::request(fixtures::MINIMUM);
    let mut receiver = service.execute_stream(request);

    while let Some(event) = receiver.recv().await {
        if event.kind == ExecutorEventKind::AgentStarted {
            break;
        }
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.active_requests() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("model request should start");

    drop(receiver);

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        model_provider.active_requests(),
        1,
        "dropping the event receiver should not cancel the running workflow"
    );

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.active_requests() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("workflow should still complete after dropping the event receiver");
}

#[tokio::test]
async fn dropping_stream_receiver_before_first_event_still_runs_workflow() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());
    let request = support::request(fixtures::MINIMUM);
    let receiver = service.execute_stream(request);

    drop(receiver);

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.recorded_count() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("workflow should still start after dropping the event receiver");
}

#[tokio::test]
async fn reconnecting_stream_replays_missed_events() {
    let model_provider = ConcurrentTrackingModelProvider::new(Duration::from_millis(50));
    let service = ExecutorService::new(model_provider);
    let mut stream_subscription = service.start_streamed_execution(support::request(fixtures::MINIMUM));
    let first_event = stream_subscription.receiver.recv().await.expect("first event should be emitted");
    let run_identifier = stream_subscription.run_identifier.clone();

    drop(stream_subscription);

    let mut reconnected_subscription = service
        .reconnect_streamed_execution(&run_identifier, Some(first_event.event_identifier))
        .expect("stream should still be available for reconnect");
    let mut event_kinds = Vec::new();

    while let Some(sequenced_event) = reconnected_subscription.receiver.recv().await {
        event_kinds.push(sequenced_event.event.kind);
    }

    assert!(!event_kinds.contains(&first_event.event.kind));
    assert!(event_kinds.contains(&ExecutorEventKind::AgentStarted));
    assert_eq!(event_kinds.last(), Some(&ExecutorEventKind::WorkflowCompleted));
}

#[tokio::test]
async fn cancelling_streamed_execution_aborts_backend_work() {
    let model_provider = ConcurrentTrackingModelProvider::new(Duration::from_secs(30));
    let service = ExecutorService::new(model_provider.clone());
    let mut stream_subscription = service.start_streamed_execution(support::request(fixtures::MINIMUM));
    let run_identifier = stream_subscription.run_identifier.clone();
    let mut event_kinds = Vec::new();

    while let Some(sequenced_event) = stream_subscription.receiver.recv().await {
        let event_kind = sequenced_event.event.kind;
        event_kinds.push(event_kind.clone());

        if event_kind == ExecutorEventKind::AgentStarted {
            break;
        }
    }

    assert!(service.cancel_streamed_execution(&run_identifier));

    while let Ok(Some(sequenced_event)) = tokio::time::timeout(Duration::from_secs(1), stream_subscription.receiver.recv()).await {
        event_kinds.push(sequenced_event.event.kind);
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        while model_provider.active_requests() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled workflow should abort active model work");

    assert!(event_kinds.contains(&ExecutorEventKind::WorkflowFailed));
}

#[tokio::test]
async fn deterministic_tool_call_emits_started_and_completed_events() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "model-a"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
            headers {
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
            model: model.openai_model
            instruction: "Summarize {{ dynamic.data }}"
            output {
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

    let mut request = request_with_input(&workflow_source, json!({ "project_id": 42, "task_id": 7 }));
    request.options.include_events = true;

    let mut receiver = service.execute_stream(request);
    let mut mcp_call_events = Vec::new();

    while let Some(event) = receiver.recv().await {
        if matches!(event.kind, ExecutorEventKind::McpCallStarted | ExecutorEventKind::McpCallCompleted) {
            mcp_call_events.push(event);
        }
    }

    assert!(
        mcp_call_events.iter().any(|event| event.kind == ExecutorEventKind::McpCallStarted),
        "expected mcp_call_started event"
    );

    assert!(
        mcp_call_events
            .iter()
            .any(|event| event.kind == ExecutorEventKind::McpCallCompleted),
        "expected mcp_call_completed event"
    );

    let started = mcp_call_events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallStarted)
        .unwrap();
    assert_eq!(started.data.as_ref().unwrap()["target_name"], "fetch_task_data");

    let completed = mcp_call_events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallCompleted)
        .unwrap();

    assert_eq!(completed.data.as_ref().unwrap()["target_name"], "fetch_task_data");

    assert_eq!(
        completed.data.as_ref().unwrap()["result"],
        json!({ "task_title": "Survey", "participants": 10 })
    );
}

#[tokio::test]
async fn output_tool_call_emits_mcp_call_events() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "__ENDPOINT__"
        }

        tool fetch_task_data from mcp.local.tool.fetch_task_data {
            bindings {
                project_id: 42
                task_id: 7
            }
        }

        output {
            data: call tool.fetch_task_data
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);
    let mut receiver = service.execute_stream(support::request(&workflow_source));
    let mut events = Vec::new();

    while let Some(event) = receiver.recv().await {
        events.push(event);
    }

    let planned_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::WorkflowPlanned)
        .expect("workflow planned event should exist");
    let validation_completed_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::McpToolValidationCompleted)
        .expect("MCP validation completion event should exist");
    let call_started_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::McpCallStarted)
        .expect("output MCP call start event should exist");
    let call_completed_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::McpCallCompleted)
        .expect("output MCP call completion event should exist");

    assert!(validation_completed_index < planned_index);
    assert!(planned_index < call_started_index);
    assert!(call_started_index < call_completed_index);

    let planned_event = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::WorkflowPlanned)
        .expect("workflow planned event should exist");
    let planned_steps = planned_event.data.as_ref().unwrap()["steps"]
        .as_array()
        .expect("planned steps should be an array");
    assert_eq!(planned_steps[0]["type"], "workflow_output");
    assert_eq!(planned_steps[0]["calls"][0]["operation"], "call");
    assert_eq!(planned_steps[0]["calls"][0]["target_name"], "fetch_task_data");

    let call_started = &events[call_started_index];
    assert_eq!(call_started.data.as_ref().unwrap()["operation"], "call");
    assert_eq!(call_started.data.as_ref().unwrap()["target_name"], "fetch_task_data");
    assert_eq!(call_started.data.as_ref().unwrap()["server_name"], "local");
    assert_eq!(call_started.data.as_ref().unwrap()["item_name"], "fetch_task_data");
    assert_eq!(call_started.data.as_ref().unwrap()["params"]["project_id"], 42);
    assert_eq!(call_started.data.as_ref().unwrap()["input_schema"]["type"], "object");

    let call_completed = &events[call_completed_index];
    assert_eq!(call_completed.data.as_ref().unwrap()["result"]["task_title"], "Survey");
    assert_eq!(
        call_completed.data.as_ref().unwrap()["raw_result"]["structuredContent"]["participants"],
        10
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mcp_tool_schema_fetch_and_validation_events_are_emitted() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "model-a"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
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
            model: model.openai_model
            instruction: "Summarize {{ dynamic.data }}"
            output {
                summary: string
            }
        }

        output {
            summary: agent.summarizer.summary
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(vec![json!({ "summary": "done" })]);
    let service = ExecutorService::new(model_provider);
    let mut receiver = service.execute_stream(request_with_input(&workflow_source, json!({ "project_id": 42, "task_id": 7 })));
    let mut events = Vec::new();

    while let Some(event) = receiver.recv().await {
        events.push(event);
    }

    let schema_started = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpToolSchemaFetchStarted)
        .expect("schema fetch start event should exist");
    assert_eq!(schema_started.data.as_ref().unwrap()["server_name"], "local");

    let schema_completed = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpToolSchemaFetchCompleted)
        .expect("schema fetch completion event should exist");
    assert_eq!(schema_completed.data.as_ref().unwrap()["server_name"], "local");
    assert_eq!(schema_completed.data.as_ref().unwrap()["tool_count"], 1);
    assert!(schema_completed.data.as_ref().unwrap()["duration_ms"].as_u64().is_some());

    let validation_started = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpToolValidationStarted)
        .expect("MCP validation start event should exist");
    assert_eq!(validation_started.agent_name.as_deref(), Some(""));
    assert_eq!(validation_started.data.as_ref().unwrap()["tool_name"], "fetch_task_data");
    assert_eq!(validation_started.data.as_ref().unwrap()["arguments"]["project_id"], 42);
    assert_eq!(validation_started.data.as_ref().unwrap()["params"]["project_id"], 42);
    assert_eq!(validation_started.data.as_ref().unwrap()["input_schema"]["type"], "object");

    let validation_completed = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpToolValidationCompleted)
        .expect("MCP validation completion event should exist");
    assert_eq!(validation_completed.agent_name.as_deref(), Some(""));
    assert_eq!(validation_completed.data.as_ref().unwrap()["tool_name"], "fetch_task_data");
    assert!(validation_completed.data.as_ref().unwrap()["duration_ms"].as_u64().is_some());

    let validation_completed_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::McpToolValidationCompleted)
        .expect("MCP validation completion event should exist");
    let planned_index = events
        .iter()
        .position(|event| event.kind == ExecutorEventKind::WorkflowPlanned)
        .expect("workflow planned event should exist");

    assert!(validation_completed_index < planned_index);

    let planned_event = &events[planned_index];
    assert_eq!(planned_event.data.as_ref().unwrap()["steps"][0]["type"], "workflow_dynamic");
    assert_eq!(planned_event.data.as_ref().unwrap()["steps"][0]["calls"][0]["operation"], "call");
    assert_eq!(
        planned_event.data.as_ref().unwrap()["steps"][0]["calls"][0]["target_name"],
        "fetch_task_data"
    );

    let mcp_call_started = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallStarted && event.data.as_ref().unwrap()["operation"] == "call")
        .expect("MCP tool call start event should exist");
    assert_eq!(mcp_call_started.data.as_ref().unwrap()["target_name"], "fetch_task_data");
    assert_eq!(mcp_call_started.data.as_ref().unwrap()["server_name"], "local");
    assert_eq!(mcp_call_started.data.as_ref().unwrap()["item_name"], "fetch_task_data");
    assert_eq!(mcp_call_started.data.as_ref().unwrap()["params"]["project_id"], 42);
    assert_eq!(mcp_call_started.data.as_ref().unwrap()["input_schema"]["type"], "object");

    let mcp_call_completed = events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallCompleted && event.data.as_ref().unwrap()["operation"] == "call")
        .expect("MCP tool call completion event should exist");
    assert_eq!(mcp_call_completed.data.as_ref().unwrap()["result"]["task_title"], "Survey");
    assert_eq!(
        mcp_call_completed.data.as_ref().unwrap()["raw_result"]["structuredContent"]["participants"],
        10
    );
}

#[tokio::test]
async fn explicit_mcp_calls_emit_started_and_completed_events() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            workspace_id: string
        }

        resource project_readme from mcp.local.resource.project_readme {
            bindings {
                workspace_id: input.workspace_id
            }
        }

        prompt system_prompt from mcp.local.prompt.system_prompt

        dynamic {
            readme: read resource.project_readme {
                bindings {
                    section: "setup"
                }
            }
            instructions: render prompt.system_prompt {
                bindings {
                    readme: dynamic.readme
                }
            }
        }

        output {
            readme: dynamic.readme
            instructions: dynamic.instructions
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TestModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);
    let mut receiver = service.execute_stream(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })));
    let mut mcp_call_events = Vec::new();

    while let Some(event) = receiver.recv().await {
        if matches!(event.kind, ExecutorEventKind::McpCallStarted | ExecutorEventKind::McpCallCompleted) {
            mcp_call_events.push(event);
        }
    }

    let started_targets = mcp_call_events
        .iter()
        .filter(|event| event.kind == ExecutorEventKind::McpCallStarted)
        .map(|event| event.data.as_ref().unwrap()["target_name"].as_str().unwrap())
        .collect::<Vec<_>>();
    let completed_targets = mcp_call_events
        .iter()
        .filter(|event| event.kind == ExecutorEventKind::McpCallCompleted)
        .map(|event| event.data.as_ref().unwrap()["target_name"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(started_targets, vec!["project_readme", "system_prompt"]);
    assert_eq!(completed_targets, vec!["project_readme", "system_prompt"]);

    let resource_started = mcp_call_events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallStarted && event.data.as_ref().unwrap()["target_name"] == "project_readme")
        .expect("resource read start event should exist");
    assert_eq!(resource_started.data.as_ref().unwrap()["operation"], "read");
    assert_eq!(resource_started.data.as_ref().unwrap()["server_name"], "local");
    assert_eq!(resource_started.data.as_ref().unwrap()["item_name"], "project_readme");
    assert_eq!(resource_started.data.as_ref().unwrap()["arguments"]["workspace_id"], "workspace-1");
    assert_eq!(resource_started.data.as_ref().unwrap()["arguments"]["section"], "setup");
    assert_eq!(resource_started.data.as_ref().unwrap()["params"]["section"], "setup");

    let prompt_completed = mcp_call_events
        .iter()
        .find(|event| event.kind == ExecutorEventKind::McpCallCompleted && event.data.as_ref().unwrap()["target_name"] == "system_prompt")
        .expect("prompt render completion event should exist");
    assert_eq!(prompt_completed.data.as_ref().unwrap()["operation"], "render");
    assert_eq!(prompt_completed.data.as_ref().unwrap()["server_name"], "local");
    assert_eq!(prompt_completed.data.as_ref().unwrap()["item_name"], "system_prompt");
    assert!(prompt_completed.data.as_ref().unwrap()["result"]
        .as_str()
        .is_some_and(|result| result.contains("Follow project conventions.")));
    assert!(
        prompt_completed.data.as_ref().unwrap()["raw_result"]["messages"][0]["content"]["text"]
            .as_str()
            .is_some_and(|result| result.contains("Follow project conventions."))
    );
}

struct TestMcpHttpServer {
    endpoint: String,
}

impl TestMcpHttpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(24) {
                let stream = incoming_stream.expect("test MCP stream should open");
                handle_mcp_request(stream);
            }
        });

        Self { endpoint }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }
}

fn handle_mcp_request(mut stream: TcpStream) {
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

    let response = if let Some(response_body) = response_for_method(request.get("method").and_then(Value::as_str)) {
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

fn response_for_method(method: Option<&str>) -> Option<Value> {
    match method {
        Some("notifications/initialized") => None,
        Some("tools/call") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "content": [{ "type": "text", "text": "{\"task_title\":\"Survey\",\"participants\":10}" }],
                "structuredContent": { "task_title": "Survey", "participants": 10 }
            }
        })),
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
                    }
                ]
            }
        })),
        Some("resources/list") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "resources": [
                    {
                        "name": "project_readme",
                        "title": "Project README",
                        "description": "The project readme file",
                        "mimeType": "text/markdown",
                        "uri": "file://resources/project_readme"
                    }
                ]
            }
        })),
        Some("resources/read") => Some(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "contents": [
                    {
                        "uri": "file://resources/project_readme",
                        "mimeType": "text/markdown",
                        "text": "# Project README\nUse stable sorting."
                    }
                ]
            }
        })),
        Some("prompts/get") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "messages": [
                    {
                        "role": "system",
                        "content": {
                            "type": "text",
                            "text": "Follow project conventions."
                        }
                    }
                ]
            }
        })),
        _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}
