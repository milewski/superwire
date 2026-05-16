use super::{read_project_mcp_lock, resolve_mcp_lock};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use superwire_core::mcp::{McpLock, McpLockResolutionContext, ProjectMcpLock};
use superwire_core::workflow_source;

#[test]
fn reads_mcp_lock_from_project_lock_without_refreshing() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
            headers {
                Accept: "application/json"
            }
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };
    let temp_directory_path = std::env::temp_dir().join(format!(
        "superwire_lsp_lock_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("current time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_directory_path).expect("temporary directory should be created");
    let temp_file_path = temp_directory_path.join("dynamic.wire");
    std::fs::write(&temp_file_path, workflow_source).expect("temporary workflow should write");
    let document_uri = format!("file://{}", temp_file_path.display());
    let lock_path = temp_directory_path.join("superwire.lock");
    let lock_context = McpLockResolutionContext {
        input: BTreeMap::new(),
        secrets: [("mcp_endpoint".to_string(), Value::String(server.endpoint()))]
            .into_iter()
            .collect(),
        dynamic: BTreeMap::new(),
        agent_outputs: BTreeMap::new(),
        agent_contexts: BTreeMap::new(),
    };
    let discovered_lock = McpLock::discover_from_workflow_with_lock_context(
        &superwire_core::dsl::parse_workflow(workflow_source).expect("workflow should parse"),
        Some(&lock_context),
    )
    .expect("MCP metadata should discover using lock context");
    let mut project_lock = ProjectMcpLock::empty();

    project_lock.insert_workflow_lock(
        temp_file_path.parent().expect("temporary workflow should have parent"),
        &temp_file_path,
        discovered_lock,
    );
    project_lock.write_to_path(&lock_path).expect("project lock should write");

    let read_lock = read_project_mcp_lock(&document_uri).expect("project lock should read");

    assert!(read_lock.servers.contains_key("local"));
    assert!(!temp_file_path.with_extension("wire.lock").exists());

    let _ = std::fs::remove_dir_all(&temp_directory_path);
}

#[test]
fn discovers_mcp_lock_from_document_when_project_lock_is_missing() {
    let server = TestMcpHttpServer::spawn();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "http://placeholder.test"
        }

        output {
            value: null
        }
    };
    let workflow_source = workflow_source.replace("http://placeholder.test", &server.endpoint());

    let discovered_lock = resolve_mcp_lock("file:///playground/document.wire", &workflow_source, None)
        .expect("MCP metadata should discover from document source");

    assert!(discovered_lock.servers.contains_key("local"));
    assert!(discovered_lock.servers["local"].find_tool_with_name("update_user_name").is_some());
}

struct TestMcpHttpServer {
    endpoint: String,
}

impl TestMcpHttpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(12) {
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
        Some("tools/list") => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [{
                    "name": "update-user-name",
                    "description": "Update user name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "user_name": { "type": "string" }
                        },
                        "required": ["user_name"]
                    }
                }]
            }
        })),
        _ => Some(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}
