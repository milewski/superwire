use super::format::format_tool_name;
use super::response::{ChatCompletionResponseExt, OpenAiChatCompletionResponse};
use super::tool::ToolCallOutcome;
use super::OpenAiModelProvider;
use crate::event::ExecutorEventKind;
use crate::model::provider::ModelProvider;
use crate::model::{ModelRequest, ModelToolDefinition, ModelToolSource, ToolCallLimitScope, ToolCallTracker};
use async_openai::types::{ChatCompletionMessageToolCall, ChatCompletionToolType, FunctionCall};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_core::mcp::{McpClientPool, McpServerConfig};
use superwire_core::semantic::support::provider::OpenAIProviderConfig;

#[test]
fn formats_tool_name_for_openai_constraints() {
    assert_eq!(format_tool_name("update user!*"), "update_user__");
}

#[test]
fn preserves_reasoning_content_on_assistant_tool_call_replay() {
    let response: OpenAiChatCompletionResponse = serde_json::from_value(json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "reasoning_content": "I need to call the tool first.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "update_user_name",
                        "arguments": "{\"user_name\":\"Ada\"}"
                    }
                }]
            }
        }]
    }))
    .expect("response should deserialize");

    let message = response
        .extract_tool_call_message()
        .expect("assistant tool call message should extract");

    assert_eq!(message.get("role"), Some(&json!("assistant")));
    assert_eq!(message.get("reasoning_content"), Some(&json!("I need to call the tool first.")));
    assert_eq!(message.get("tool_calls").and_then(Value::as_array).map(Vec::len), Some(1));
}

#[tokio::test]
async fn replays_reasoning_content_after_tool_call() {
    let provider = OpenAiModelProvider;
    let model_server = TestOpenAiHttpServer::spawn(vec![
        json!({
            "id": "chatcmpl_1",
            "object": "chat.completion",
            "created": 1,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "reasoning_content": "I should update the user name before answering.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "update_user_name",
                            "arguments": "{\"user_name\":\"Ada\"}"
                        }
                    }]
                }
            }]
        }),
        json!({
            "id": "chatcmpl_2",
            "object": "chat.completion",
            "created": 2,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_2",
                        "type": "function",
                        "function": {
                            "name": "finalize",
                            "arguments": "{\"type\":\"success\",\"output\":{\"success\":true}}"
                        }
                    }]
                }
            }]
        }),
    ]);
    let mcp_server = TestMcpHttpServer::spawn();
    let request = model_request(model_server.endpoint(), mcp_server.endpoint());

    let response = provider.generate(request).await.expect("model should complete");

    assert_eq!(response.output, json!({ "success": true }));
    let requests = model_server.requests();
    let second_request = requests.get(1).expect("second chat completion request should be sent");
    let assistant_message = second_request
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.iter().find(|message| message.get("role") == Some(&json!("assistant"))))
        .expect("assistant tool call message should be replayed");

    assert_eq!(
        assistant_message.get("reasoning_content"),
        Some(&json!("I should update the user name before answering."))
    );
}

#[test]
fn executes_mcp_tool_call_from_model_request() {
    let provider = OpenAiModelProvider;
    let server = TestMcpHttpServer::spawn();
    let request = model_request("https://api.openai.com/v1".to_string(), server.endpoint());
    let tool_call = ChatCompletionMessageToolCall {
        id: "call_1".to_string(),
        r#type: ChatCompletionToolType::Function,
        function: FunctionCall {
            name: "update_user_name".to_string(),
            arguments: serde_json::json!({ "user_id": 999, "user_name": "Ada" }).to_string(),
        },
    };

    let result = continue_result(
        provider
            .execute_tool_call(&request, &tool_call)
            .expect("MCP tool call should execute"),
    );

    assert_eq!(result, serde_json::json!({ "success": true }));
    assert_eq!(
        server.received_tool_arguments(),
        Some(serde_json::json!({
            "project_id": 14,
            "user_id": 123,
            "user_name": "Ada"
        }))
    );
}

#[test]
fn rejects_invalid_tool_arguments_before_mcp_call() {
    let provider = OpenAiModelProvider;
    let server = TestMcpHttpServer::spawn();
    let mut request = model_request("https://api.openai.com/v1".to_string(), server.endpoint());
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(4);

    request.event_sender = Some(event_sender);
    request.tools[0].input_schema = json!({
        "type": "object",
        "properties": {
            "primary_language": {
                "type": "string",
                "enum": ["en_US", "es", "fr"]
            },
            "languages": {
                "type": "array",
                "items": {
                    "type": "string",
                    "enum": ["en_US", "es", "fr"]
                }
            }
        },
        "required": ["primary_language", "languages"],
        "additionalProperties": false
    });
    let tool_call = ChatCompletionMessageToolCall {
        id: "call_1".to_string(),
        r#type: ChatCompletionToolType::Function,
        function: FunctionCall {
            name: "update_user_name".to_string(),
            arguments: json!({
                "primary_language": "en",
                "languages": ["en", "es", "fr"]
            })
            .to_string(),
        },
    };

    let result = continue_result(
        provider
            .execute_tool_call(&request, &tool_call)
            .expect("invalid tool arguments should be returned as a tool result"),
    );

    assert_eq!(result["error"], "tool_argument_schema_mismatch");
    assert_eq!(result["tool_name"], "update_user_name");
    assert!(result["message"]
        .as_str()
        .expect("error message should be string")
        .contains("primary_language"));
    assert!(result["message"]
        .as_str()
        .expect("error message should be string")
        .contains("en_US"));
    assert_eq!(server.received_tool_arguments(), None);

    let event = event_receiver.try_recv().expect("tool call failure event should be emitted");

    assert_eq!(event.kind, ExecutorEventKind::ToolCallFailed);
    assert_eq!(event.agent_name.as_deref(), Some("updater"));
    assert_eq!(
        event.data.as_ref().and_then(|data| data.get("tool_name")),
        Some(&json!("update_user_name"))
    );
    assert_eq!(
        event.data.as_ref().and_then(|data| data.pointer("/error/error")),
        Some(&json!("tool_argument_schema_mismatch"))
    );
    assert!(
        event_receiver.try_recv().is_err(),
        "tool_call_started should not be emitted for invalid arguments"
    );
}

#[test]
fn rejects_schema_mismatched_tool_call_payload_and_never_dispatches_mcp_call() {
    let provider = OpenAiModelProvider;
    let server = TestMcpHttpServer::spawn();
    let mut request = model_request("https://api.openai.com/v1".to_string(), server.endpoint());

    request.tools[0].input_schema = json!({
        "properties": {
            "name": {
                "items": {
                    "properties": {
                        "language": {
                            "enum": ["en_US", "es", "fr"],
                            "type": "string"
                        },
                        "value": {
                            "type": "string"
                        }
                    },
                    "required": ["language", "value"],
                    "type": "object"
                },
                "minItems": 1,
                "type": ["array", "null"],
                "uniqueItems": true
            },
            "primary_language": {
                "enum": ["en_US", "es", "fr"],
                "type": ["string", "null"]
            },
            "languages": {
                "items": {
                    "enum": ["en_US", "es", "fr"],
                    "type": "string"
                },
                "minItems": 1,
                "type": ["array", "null"],
                "uniqueItems": true
            },
            "description": {
                "items": {
                    "properties": {
                        "language": {
                            "enum": ["en_US", "es", "fr"],
                            "type": "string"
                        },
                        "value": {
                            "type": "string"
                        }
                    },
                    "required": ["language", "value"],
                    "type": "object"
                },
                "minItems": 1,
                "type": ["array", "null"]
            },
            "project_id": {
                "type": "integer"
            }
        },
        "required": ["name", "primary_language", "languages", "description", "project_id"],
        "type": "object",
        "additionalProperties": false
    });
    let tool_call = ChatCompletionMessageToolCall {
        id: "call_1".to_string(),
        r#type: ChatCompletionToolType::Function,
        function: FunctionCall {
            name: "update_user_name".to_string(),
            arguments: json!({
                "description": "Soccer project",
                "languages": "en_US",
                "name": "Soccer Project",
                "primary_language": "en_US",
                "project_id": 31,
                "workspace_id": 1
            })
            .to_string(),
        },
    };

    let result = continue_result(
        provider
            .execute_tool_call(&request, &tool_call)
            .expect("schema mismatches should return a tool validation error"),
    );

    assert_eq!(result["error"], "tool_argument_schema_mismatch");
    assert_eq!(result["tool_name"], "update_user_name");

    let validation_message = result["message"].as_str().expect("validation error message should be a string");

    assert!(validation_message.contains("name") || validation_message.contains("languages"));
    assert!(validation_message.contains("workspace_id") || validation_message.contains("Additional properties"));
    assert!(validation_message.contains("declared schema"));

    assert_eq!(
        result["expected_schema"].pointer("/properties/name/type"),
        Some(&json!(["array", "null"]))
    );
    assert_eq!(
        result["expected_schema"].pointer("/properties/languages/type"),
        Some(&json!(["array", "null"]))
    );
    assert_eq!(server.received_tool_arguments(), None);
}

#[test]
fn returns_tool_error_when_max_calls_limit_is_exceeded() {
    let mcp_server = TestMcpHttpServer::spawn();
    let provider = OpenAiModelProvider;
    let mut request = model_request("http://localhost:1234/v1".to_string(), mcp_server.endpoint());
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(4);

    request.tools[0].max_calls = Some(1);
    request.event_sender = Some(event_sender);

    let tool_call = ChatCompletionMessageToolCall {
        id: "call_1".to_string(),
        r#type: ChatCompletionToolType::Function,
        function: FunctionCall {
            name: format_tool_name("update_user_name"),
            arguments: "{}".to_string(),
        },
    };

    let _ = provider
        .execute_tool_call(&request, &tool_call)
        .expect("first tool call should succeed");

    let result = continue_result(
        provider
            .execute_tool_call(&request, &tool_call)
            .expect("max_calls error should be returned as a tool result"),
    );

    assert_eq!(result["error"], "tool_call_limit_exceeded");
    assert_eq!(result["tool_name"], "update_user_name");
    assert_eq!(result["max_calls"], 1);
    assert!(result["message"]
        .as_str()
        .expect("error message should be string")
        .contains("tool `update_user_name` cannot be called more than 1 times"));

    let started_event = event_receiver.try_recv().expect("first tool call should emit started event");

    assert_eq!(started_event.kind, ExecutorEventKind::ToolCallStarted);

    let completed_event = event_receiver.try_recv().expect("first tool call should emit completed event");

    assert_eq!(completed_event.kind, ExecutorEventKind::ToolCallCompleted);

    let failed_event = event_receiver.try_recv().expect("max_calls rejection should emit failure event");

    assert_eq!(failed_event.kind, ExecutorEventKind::ToolCallFailed);
    assert_eq!(failed_event.agent_name.as_deref(), Some("updater"));
    assert_eq!(
        failed_event.data.as_ref().and_then(|data| data.pointer("/error/error")),
        Some(&json!("tool_call_limit_exceeded"))
    );
}

#[tokio::test]
async fn retries_after_max_calls_tool_error() {
    let provider = OpenAiModelProvider;
    let model_server = TestOpenAiHttpServer::spawn(vec![
        json!({
            "id": "chatcmpl_1",
            "object": "chat.completion",
            "created": 1,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "update_user_name",
                            "arguments": "{}"
                        }
                    }]
                }
            }]
        }),
        json!({
            "id": "chatcmpl_2",
            "object": "chat.completion",
            "created": 2,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_2",
                        "type": "function",
                        "function": {
                            "name": "update_user_name",
                            "arguments": "{}"
                        }
                    }]
                }
            }]
        }),
        json!({
            "id": "chatcmpl_3",
            "object": "chat.completion",
            "created": 3,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_3",
                        "type": "function",
                        "function": {
                            "name": "finalize",
                            "arguments": "{\"type\":\"success\",\"output\":{\"success\":true}}"
                        }
                    }]
                }
            }]
        }),
    ]);
    let mcp_server = TestMcpHttpServer::spawn();
    let mut request = model_request(model_server.endpoint(), mcp_server.endpoint());

    request.tools[0].max_calls = Some(1);

    let response = provider
        .generate(request)
        .await
        .expect("model should recover after max_calls tool error");

    assert_eq!(response.output, json!({ "success": true }));

    let requests = model_server.requests();

    assert_eq!(requests.len(), 3);

    let third_request = requests.get(2).expect("third request should be sent after max_calls error");
    let limit_error_message = third_request
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.iter().rev().find(|message| message.get("role") == Some(&json!("tool"))))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .expect("max_calls tool error should be replayed as tool content");

    assert!(limit_error_message.contains("tool_call_limit_exceeded"));
    assert!(limit_error_message.contains("cannot be called more than 1 times"));
}

#[test]
fn agent_scoped_max_calls_is_not_shared_across_agents() {
    let shared_tool_call_tracker = ToolCallTracker::default();
    let first_agent_scope = ToolCallLimitScope::Agent {
        agent_name: "first_agent".to_string(),
    };
    let second_agent_scope = ToolCallLimitScope::Agent {
        agent_name: "second_agent".to_string(),
    };

    shared_tool_call_tracker
        .register_call("update_user_name", Some(1), &first_agent_scope)
        .expect("first agent should be allowed to call the tool once");

    shared_tool_call_tracker
        .register_call("update_user_name", Some(1), &second_agent_scope)
        .expect("second agent should be allowed to call the tool once with independent agent scope");

    let first_agent_error = shared_tool_call_tracker
        .register_call("update_user_name", Some(1), &first_agent_scope)
        .expect_err("first agent second call should fail at max_calls limit");

    assert!(first_agent_error.contains("tool `update_user_name` cannot be called more than 1 times"));
}

fn model_request(model_endpoint: String, mcp_endpoint: String) -> ModelRequest {
    let mcp_pool = McpClientPool::from_server_configs([McpServerConfig {
        name: "local".to_string(),
        endpoint: mcp_endpoint,
        headers: [("Authorization".to_string(), "Bearer test-token".to_string())].into(),
    }])
    .expect("test mcp pool should initialize");

    ModelRequest {
        agent_name: "updater".to_string(),
        provider_config: OpenAIProviderConfig {
            endpoint: model_endpoint,
            api_key: "test-api-key".to_string(),
        },
        model_name: "deepseek-reasoner".to_string(),
        inference: BTreeMap::new(),
        prompt: "Rename the user".to_string(),
        output_schema: serde_json::json!({ "type": "object" }),
        tools: vec![
            ModelToolDefinition {
                name: "update_user_name".to_string(),
                description: Some("Update a user name".to_string()),
                source: ModelToolSource::Mcp {
                    server_name: Some("local".to_string()),
                    tool_name: "update-user-name".to_string(),
                    endpoint: String::new(),
                    headers: BTreeMap::new(),
                },
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
                bindings: serde_json::json!({ "project_id": 14, "user_id": 123 }),
                max_calls: None,
                max_calls_scope: ToolCallLimitScope::Workflow,
            },
            ModelToolDefinition::finalize(serde_json::json!({ "type": "object" })),
        ],
        event_sender: None,
        mcp_pool,
        tool_call_tracker: ToolCallTracker::default(),
    }
}

fn continue_result(tool_call_outcome: ToolCallOutcome) -> Value {
    match tool_call_outcome {
        ToolCallOutcome::Continue(value) => value,
        ToolCallOutcome::Finalized(_) => panic!("expected regular tool result"),
    }
}

struct TestMcpHttpServer {
    endpoint: String,
    received_tool_arguments: Arc<Mutex<Option<Value>>>,
}

struct TestOpenAiHttpServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl TestOpenAiHttpServer {
    fn spawn(responses: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test OpenAI listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let responses = Arc::new(Mutex::new(responses));
        let thread_responses = Arc::clone(&responses);

        thread::spawn(move || {
            let response_count = thread_responses.lock().expect("responses lock should not be poisoned").len();

            for incoming_stream in listener.incoming().take(response_count) {
                let stream = incoming_stream.expect("test OpenAI stream should open");
                handle_openai_request(stream, &thread_requests, &thread_responses);
            }
        });

        Self { endpoint, requests }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("requests lock should not be poisoned").clone()
    }
}

impl TestMcpHttpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let received_tool_arguments = Arc::new(Mutex::new(None));
        let thread_received_tool_arguments = Arc::clone(&received_tool_arguments);

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(3) {
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
    let mut request_headers = BTreeMap::new();
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

        if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
            request_headers.insert(header_name.to_ascii_lowercase(), header_value.trim().to_string());
        }
    }

    assert_eq!(
        request_headers.get("authorization"),
        Some(&"Bearer test-token".to_string()),
        "expected MCP request authorization header"
    );

    let mut request_body = vec![0_u8; content_length];
    reader.read_exact(&mut request_body).expect("request body should read");
    let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");

    if request.get("method").and_then(Value::as_str) == Some("tools/call") {
        *received_tool_arguments
            .lock()
            .expect("received tool arguments lock should not be poisoned") =
            request.get("params").and_then(|params| params.get("arguments")).cloned();
    }

    let response = if let Some(response_body) = response_for_method(request.get("method").and_then(Value::as_str)) {
        let response_body = response_body.to_string();

        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        )
    } else {
        "HTTP/1.1 202 Accepted\r\nconnection: close\r\ncontent-length: 0\r\n\r\n".to_string()
    };

    stream.write_all(response.as_bytes()).expect("response should write");
}

fn handle_openai_request(mut stream: TcpStream, requests: &Arc<Mutex<Vec<Value>>>, responses: &Arc<Mutex<Vec<Value>>>) {
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

    requests.lock().expect("requests lock should not be poisoned").push(request);

    let response_body = responses
        .lock()
        .expect("responses lock should not be poisoned")
        .remove(0)
        .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );

    stream.write_all(response.as_bytes()).expect("response should write");
}

fn response_for_method(method: Option<&str>) -> Option<Value> {
    match method {
        Some("notifications/initialized") => None,
        Some("tools/call") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "content": [{ "type": "text", "text": "{\"success\":true}" }],
                "structuredContent": { "success": true }
            }
        })),
        _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}
