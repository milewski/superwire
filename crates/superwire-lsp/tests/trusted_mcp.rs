#[macro_use]
mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use common::{
    assert_publish_diagnostics_for_uri, character_index_for_fragment, did_open_params, line_index_with_fragments,
    text_document_position_params, LspProcessClient,
};
use serde_json::{json, Value};

const DEFAULT_DOCUMENT_URI: &str = "file:///workspace/default-mcp.wire";
const TRUSTED_DOCUMENT_URI: &str = "file:///workspace/trusted-mcp.wire";

#[derive(Debug, Clone)]
struct ObservedMcpRequest {
    method: String,
    authorization: Option<String>,
}

#[derive(Debug)]
struct MockMcpServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<ObservedMcpRequest>>>,
    should_stop: Arc<AtomicBool>,
    server_thread: Option<JoinHandle<()>>,
}

impl MockMcpServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock MCP listener should bind");
        listener.set_nonblocking(true).expect("mock MCP listener should become nonblocking");
        let socket_address = listener.local_addr().expect("mock MCP listener should have an address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker_requests = Arc::clone(&requests);
        let should_stop = Arc::new(AtomicBool::new(false));
        let worker_should_stop = Arc::clone(&should_stop);
        let server_thread = std::thread::Builder::new()
            .name("superwire-lsp-test-mcp-http".to_string())
            .spawn(move || {
                while !worker_should_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _peer_address)) => Self::handle_connection(stream, &worker_requests),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("mock MCP listener failed: {error}"),
                    }
                }
            })
            .expect("mock MCP server thread should start");

        Self {
            endpoint: format!("http://{socket_address}/mcp"),
            requests,
            should_stop,
            server_thread: Some(server_thread),
        }
    }

    fn requests(&self) -> Vec<ObservedMcpRequest> {
        self.requests.lock().expect("mock MCP request log should not be poisoned").clone()
    }

    fn handle_connection(mut stream: TcpStream, requests: &Mutex<Vec<ObservedMcpRequest>>) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("mock MCP stream read timeout should configure");
        let mut request_bytes = Vec::new();
        let mut temporary_buffer = [0_u8; 4096];
        let (header_end_offset, content_length) = loop {
            let bytes_read = stream.read(&mut temporary_buffer).expect("mock MCP request should read");
            assert_ne!(bytes_read, 0, "mock MCP request should include headers");
            request_bytes.extend_from_slice(&temporary_buffer[..bytes_read]);

            let Some(header_end_offset) = request_bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let header_text = String::from_utf8_lossy(&request_bytes[..header_end_offset]);
            let content_length = header_text
                .lines()
                .filter_map(|header_line| header_line.split_once(':'))
                .find(|(header_name, _header_value)| header_name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_header_name, header_value)| header_value.trim().parse::<usize>().ok())
                .expect("mock MCP request should include content length");

            break (header_end_offset, content_length);
        };
        let body_start_offset = header_end_offset + 4;

        while request_bytes.len() < body_start_offset + content_length {
            let bytes_read = stream.read(&mut temporary_buffer).expect("mock MCP request body should read");
            assert_ne!(bytes_read, 0, "mock MCP request body should be complete");
            request_bytes.extend_from_slice(&temporary_buffer[..bytes_read]);
        }

        let header_text = String::from_utf8_lossy(&request_bytes[..header_end_offset]);
        let authorization =
            header_text
                .lines()
                .filter_map(|header_line| header_line.split_once(':'))
                .find_map(|(header_name, header_value)| {
                    header_name
                        .eq_ignore_ascii_case("authorization")
                        .then(|| header_value.trim().to_string())
                });
        let request_body: Value = serde_json::from_slice(&request_bytes[body_start_offset..body_start_offset + content_length])
            .expect("mock MCP request body should be JSON");
        let method = request_body.get("method").and_then(Value::as_str).unwrap_or_default().to_string();

        requests
            .lock()
            .expect("mock MCP request log should not be poisoned")
            .push(ObservedMcpRequest {
                method: method.clone(),
                authorization,
            });

        if method == "notifications/initialized" {
            stream
                .write_all(b"HTTP/1.1 202 Accepted\r\nconnection: close\r\ncontent-length: 0\r\n\r\n")
                .expect("mock MCP notification response should write");

            return;
        }

        let result = match method.as_str() {
            "tools/list" => json!({
                "tools": [{
                    "name": "schema_tool",
                    "description": "Tool exposed by the stdio integration test",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "query": { "type": "string" } },
                        "required": ["query"]
                    },
                    "outputSchema": {
                        "type": "object",
                        "properties": { "accepted": { "type": "boolean" } },
                        "required": ["accepted"]
                    }
                }]
            }),
            "resources/list" => json!({ "resources": [] }),
            "resources/templates/list" => json!({ "resourceTemplates": [] }),
            "prompts/list" => json!({ "prompts": [] }),
            _ => json!({}),
        };
        let response_body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": request_body.get("id").cloned().unwrap_or_else(|| json!(1)),
            "result": result
        }))
        .expect("mock MCP response should serialize");
        let response_header = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n",
            response_body.len()
        );

        stream
            .write_all(response_header.as_bytes())
            .expect("mock MCP response header should write");
        stream.write_all(&response_body).expect("mock MCP response body should write");
    }
}

impl Drop for MockMcpServer {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Release);

        if let Some(server_thread) = self.server_thread.take() {
            server_thread.join().expect("mock MCP server thread should stop");
        }
    }
}

#[tokio::test]
async fn default_stdio_initialization_never_contacts_workflow_mcp_endpoint() {
    let mock_mcp_server = MockMcpServer::start();
    let document_text = mcp_document_text(&mock_mcp_server.endpoint);
    let mut language_server_client = LspProcessClient::spawn();
    let initialize_response = language_server_client
        .send_request(1, "initialize", json!({ "capabilities": {} }))
        .await;

    assert_eq!(
        initialize_response["result"]["capabilities"]["experimental"]["superwire"]["initializationOptions"]["workspaceTrust"]
            ["networkMcpDiscovery"]["default"],
        "disabled"
    );

    language_server_client
        .send_notification("textDocument/didOpen", did_open_params(DEFAULT_DOCUMENT_URI, &document_text))
        .await;
    let _diagnostics = assert_publish_diagnostics_for_uri(&mut language_server_client, DEFAULT_DOCUMENT_URI).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(mock_mcp_server.requests().is_empty());

    shutdown_server(&mut language_server_client, 2).await;
}

#[tokio::test]
async fn trusted_stdio_initialization_discovers_real_http_mcp_metadata() {
    let mock_mcp_server = MockMcpServer::start();
    let document_text = mcp_document_text(&mock_mcp_server.endpoint);
    let mut language_server_client = LspProcessClient::spawn();
    let initialize_response = language_server_client
        .send_request(
            10,
            "initialize",
            json!({
                "capabilities": {},
                "initializationOptions": {
                    "workspaceTrust": {
                        "networkMcpDiscovery": "trusted"
                    }
                }
            }),
        )
        .await;

    assert!(initialize_response["result"].is_object());

    language_server_client
        .send_notification("textDocument/didOpen", did_open_params(TRUSTED_DOCUMENT_URI, &document_text))
        .await;
    let _initial_diagnostics = assert_publish_diagnostics_for_uri(&mut language_server_client, TRUSTED_DOCUMENT_URI).await;
    let accepted_diagnostics = assert_publish_diagnostics_for_uri(&mut language_server_client, TRUSTED_DOCUMENT_URI).await;

    assert_eq!(accepted_diagnostics["params"]["version"], 1);
    assert_eq!(accepted_diagnostics["params"]["diagnostics"], json!([]));

    let import_line = line_index_with_fragments(&document_text, &["tool", "imported", "mcp", "schema_tool"]);
    let model_reference_line = line_index_with_fragments(&document_text, &["model", "model.fast"]);
    let schema_tool_character = character_index_for_fragment(&document_text, import_line, "schema_tool");
    let model_reference_character = character_index_for_fragment(&document_text, model_reference_line, "fast");
    let completion_response = language_server_client
        .send_request(
            11,
            "textDocument/completion",
            text_document_position_params(TRUSTED_DOCUMENT_URI, import_line, schema_tool_character),
        )
        .await;
    let completion_labels = completion_response["result"]["items"]
        .as_array()
        .expect("trusted completion response should contain items")
        .iter()
        .filter_map(|completion_item| completion_item["label"].as_str())
        .collect::<Vec<_>>();

    assert!(completion_labels.contains(&"schema_tool"));

    let hover_response = language_server_client
        .send_request(
            12,
            "textDocument/hover",
            text_document_position_params(TRUSTED_DOCUMENT_URI, model_reference_line, model_reference_character),
        )
        .await;
    let hover_markdown = hover_response["result"]["contents"]["value"]
        .as_str()
        .expect("trusted document model should provide hover markdown");

    assert!(hover_markdown.contains("Provider"));

    let observed_requests = mock_mcp_server.requests();

    assert!(observed_requests.iter().any(|request| request.method == "tools/list"));
    assert!(observed_requests
        .iter()
        .all(|request| request.authorization.as_deref() == Some("Bearer stdio-secret")));

    shutdown_server(&mut language_server_client, 13).await;
}

fn mcp_document_text(endpoint: &str) -> String {
    dsl! {
        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1"
        }

        mcp local {
            endpoint: "__MCP_ENDPOINT__"
            headers {
                Authorization: "Bearer stdio-secret"
            }
        }

        tool imported from mcp.local.tool.schema_tool

        agent tooling {
            model: model.fast
            instruction: "Use the imported tool"
            uses: [tool.imported]
            output {
                accepted: boolean
            }
        }

        output {
            value: null
        }
    }
    .replace("__MCP_ENDPOINT__", endpoint)
}

async fn shutdown_server(language_server_client: &mut LspProcessClient, request_id: u64) {
    let shutdown_response = language_server_client.send_request(request_id, "shutdown", Value::Null).await;

    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.send_notification("exit", Value::Null).await;
    language_server_client.wait_for_exit().await;
}
