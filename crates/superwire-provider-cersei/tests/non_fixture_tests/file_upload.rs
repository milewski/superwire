use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_mcp::McpClientPool;
use superwire_model::{ModelFileAttachment, ModelProvider, ModelRequest, ModelSchema, ModelToolDefinition, ToolCallTracker};
use superwire_protocol::event::{ExecutorEvent, ExecutorEventKind};
use superwire_provider_cersei::CerseiModelProvider;
use superwire_semantic::support::provider::{ProviderConfig, ProviderDriver};
use superwire_types::ModelWireApi;
use tokio::sync::mpsc;

#[tokio::test]
async fn uploads_file_injects_file_id_message_and_deletes_file() {
    let server = FileProviderServer::spawn(ChatResponseMode::Success);
    let (event_sender, mut event_receiver) = mpsc::channel(8);
    let request = model_request(server.endpoint.clone(), Some(event_sender));

    let response = CerseiModelProvider.generate(request).await.expect("provider should complete");

    assert_eq!(response.output, json!({ "value": "done" }));

    let requests = server.requests();
    assert!(requests
        .iter()
        .any(|request| request.method == "POST" && request.path == "/v1/files"));
    assert!(requests
        .iter()
        .any(|request| request.method == "DELETE" && request.path == "/v1/files/file-fe-test"));

    let chat_request = requests
        .iter()
        .find(|request| request.method == "POST" && request.path == "/v1/chat/completions")
        .expect("chat request should be recorded");
    let messages = chat_request.body_json["messages"].as_array().expect("messages should be an array");

    assert!(messages.iter().any(|message| {
        message.get("role").and_then(Value::as_str) == Some("system")
            && message.get("content").and_then(Value::as_str) == Some("fileid://file-fe-test")
    }));

    let created_event = event_receiver.recv().await.expect("file created event should be sent");
    let deleted_event = event_receiver.recv().await.expect("file deleted event should be sent");

    assert_eq!(created_event.kind, ExecutorEventKind::AgentFileCreated);
    assert_eq!(created_event.agent_name.as_deref(), Some("reviewer"));
    assert_eq!(
        created_event.data.as_ref().and_then(|data| data["file_id"].as_str()),
        Some("file-fe-test")
    );
    assert_eq!(
        created_event.data.as_ref().and_then(|data| data["filename"].as_str()),
        Some("example.json")
    );
    assert_eq!(
        created_event.data.as_ref().and_then(|data| data["purpose"].as_str()),
        Some("file-extract")
    );
    assert_eq!(created_event.data.as_ref().and_then(|data| data["bytes"].as_u64()), Some(19));

    assert_eq!(deleted_event.kind, ExecutorEventKind::AgentFileDeleted);
    assert_eq!(deleted_event.agent_name.as_deref(), Some("reviewer"));
    assert_eq!(
        deleted_event.data.as_ref().and_then(|data| data["file_id"].as_str()),
        Some("file-fe-test")
    );
}

#[tokio::test]
async fn deletes_file_when_chat_completion_fails() {
    let server = FileProviderServer::spawn(ChatResponseMode::Failure);
    let mut request = model_request(server.endpoint.clone(), None);
    request.inference.insert("provider_max_retries".to_string(), json!(0));

    let response = CerseiModelProvider.generate(request).await;

    assert!(response.is_err());

    let requests = server.requests();
    assert!(requests
        .iter()
        .any(|request| request.method == "POST" && request.path == "/v1/files"));
    assert!(requests
        .iter()
        .any(|request| request.method == "POST" && request.path == "/v1/chat/completions"));
    assert!(requests
        .iter()
        .any(|request| request.method == "DELETE" && request.path == "/v1/files/file-fe-test"));
}

fn model_request(endpoint: String, event_sender: Option<mpsc::Sender<ExecutorEvent>>) -> ModelRequest {
    ModelRequest {
        agent_name: "reviewer".to_string(),
        provider_config: ProviderConfig {
            driver: ProviderDriver::OpenAiCompatible,
            endpoint: Some(endpoint),
            api_key: Some("test-api-key".to_string()),
        },
        model_name: "qwen-doc-turbo".to_string(),
        wire_api: ModelWireApi::ChatCompletion,
        inference: HashMap::new(),
        context: None,
        prompt: "What is this file about?".to_string(),
        prompt_content: Vec::new(),
        file_attachments: vec![ModelFileAttachment {
            name: "example.json".to_string(),
            content: "{\"value\":\"content\"}".to_string(),
            purpose: "file-extract".to_string(),
        }],
        output_schema: ModelSchema::OpenObject,
        tools: vec![ModelToolDefinition::finalize(ModelSchema::OpenObject)],
        event_sender,
        mcp_pool: McpClientPool::empty(),
        tool_call_tracker: ToolCallTracker::default(),
    }
}

#[derive(Clone)]
struct FileProviderServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

#[derive(Clone, Copy)]
enum ChatResponseMode {
    Success,
    Failure,
}

impl ChatResponseMode {
    fn expected_request_count(self) -> usize {
        match self {
            Self::Success | Self::Failure => 3,
        }
    }
}

#[derive(Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    body_json: Value,
}

impl FileProviderServer {
    fn spawn(chat_response_mode: ChatResponseMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("file provider listener should bind");
        let endpoint = format!("http://{}/v1", listener.local_addr().expect("local address should exist"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(chat_response_mode.expected_request_count()) {
                let mut stream = incoming_stream.expect("provider stream should open");
                let request = read_http_request(&stream).expect("request should parse");
                let response = response_for_request(&request, chat_response_mode);

                thread_requests.lock().expect("requests lock should not be poisoned").push(request);
                stream.write_all(response.as_bytes()).expect("response should write");
            }
        });

        Self { endpoint, requests }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests lock should not be poisoned").clone()
    }
}

fn read_http_request(stream: &TcpStream) -> Option<RecordedRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
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
    let body_text = String::from_utf8_lossy(&body);
    let body_json = serde_json::from_str(&body_text).unwrap_or(Value::Null);

    Some(RecordedRequest { method, path, body_json })
}

fn response_for_request(request: &RecordedRequest, chat_response_mode: ChatResponseMode) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/v1/files") => http_json_response(json!({
            "id": "file-fe-test",
            "bytes": 19,
            "created_at": 1_729_065_448,
            "filename": "example.json",
            "object": "file",
            "purpose": "file-extract",
            "status": "processed",
            "status_details": null
        })),
        ("DELETE", "/v1/files/file-fe-test") => http_json_response(json!({
            "id": "file-fe-test",
            "deleted": true,
            "object": "file"
        })),
        ("POST", "/v1/chat/completions") => match chat_response_mode {
            ChatResponseMode::Success => http_sse_response(),
            ChatResponseMode::Failure => http_error_response(),
        },
        _ => http_json_response(json!({ "error": { "message": "unexpected request" } })),
    }
}

fn http_error_response() -> String {
    let body_text = json!({ "error": { "message": "chat failed" } }).to_string();

    format!(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body_text.len(),
        body_text
    )
}

fn http_json_response(body: Value) -> String {
    let body_text = body.to_string();

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body_text.len(),
        body_text
    )
}

fn http_sse_response() -> String {
    let event = json!({
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "finalize",
                        "arguments": "{\"type\":\"success\",\"output\":{\"value\":\"done\"}}"
                    }
                }]
            },
            "finish_reason": null
        }]
    });
    let body_text = format!("data: {event}\n\ndata: [DONE]\n\n");

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
        body_text.len(),
        body_text
    )
}
