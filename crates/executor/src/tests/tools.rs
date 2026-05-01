use crate::model::ModelToolSource;
use crate::service::ExecutorService;
use crate::tests::support::{request, TrackingModelProvider};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use superwire_core::workflow_source;

#[tokio::test]
async fn agent_tool_definitions_are_passed_to_model_provider() {
    let server = TestMcpHttpServer::spawn([("authorization".to_string(), "Bearer test-token".to_string())]);
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        mcp local {
            endpoint: "__ENDPOINT__"
            headers: {
                Authorization: "Bearer test-token"
            }
        }

        input {
            user_id: number
        }

        tool local_update_user {
            description: "Update a user name"
            using: mcp.local.update_user_name

            bindings {
                user_id: input.user_id
            }

            input {
                user_name: string
            }
        }

        agent updater {
            model: openai("gpt-4.1-mini")
            tools: [tool.local_update_user]
            prompt: "Rename the user"
            output: string
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TrackingModelProvider::new(vec![serde_json::json!("renamed")]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, serde_json::json!({ "user_id": 123 })))
        .await
        .expect("execution should pass tool metadata to provider");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");
    let tool_definition = request.tools.first().expect("tool definition should be present");

    assert_eq!(tool_definition.name, "local_update_user");
    assert_eq!(tool_definition.description.as_deref(), Some("Update a user name"));
    assert_eq!(
        tool_definition.source,
        ModelToolSource::Mcp {
            server_name: Some("local".to_string()),
            tool_name: "update_user_name".to_string(),
            endpoint: server.endpoint(),
            headers: [("Authorization".to_string(), "Bearer test-token".to_string())].into(),
        }
    );
    assert_eq!(tool_definition.bindings, serde_json::json!({ "user_id": 123 }));
    assert_eq!(tool_definition.input_schema["required"], serde_json::json!(["user_name"]));
    assert_eq!(tool_definition.input_schema.pointer("/properties/user_id"), None);
    assert_eq!(tool_definition.output_schema["required"], serde_json::json!(["success"]));
}

struct TestMcpHttpServer {
    endpoint: String,
}

impl TestMcpHttpServer {
    fn spawn(expected_headers: impl IntoIterator<Item = (String, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let expected_headers = expected_headers.into_iter().collect::<BTreeMap<_, _>>();

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(3) {
                let stream = incoming_stream.expect("test MCP stream should open");
                handle_mcp_request(stream, &expected_headers);
            }
        });

        Self { endpoint }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }
}

fn handle_mcp_request(mut stream: TcpStream, expected_headers: &BTreeMap<String, String>) {
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

    for (header_name, header_value) in expected_headers {
        assert_eq!(
            request_headers.get(header_name),
            Some(header_value),
            "expected MCP request header `{header_name}`"
        );
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
        Some("tools/list") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [{
                    "name": "update_user_name",
                    "description": "Update a user name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "user_id": { "type": "number" },
                            "user_name": { "type": "string" }
                        },
                        "required": ["user_id", "user_name"]
                    },
                    "outputSchema": {
                        "type": "object",
                        "properties": { "success": { "type": "boolean" } },
                        "required": ["success"]
                    }
                }]
            }
        })),
        _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}

fn request_with_input(fixture: &str, input: serde_json::Value) -> crate::api::ExecutionRequest {
    let mut execution_request = request(fixture);
    execution_request.input = input;

    execution_request
}
