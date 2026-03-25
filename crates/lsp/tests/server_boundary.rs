use std::{path::PathBuf, process::Stdio, time::Duration};

macro_rules! dsl {
    ($($tokens:tt)*) => {{
        let source = stringify!($($tokens)*);
        let trimmed = source.trim_start_matches('\n').trim_end_matches(['\n', ' ']);
        trimmed.to_string()
    }};
}

use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

struct LspProcessClient {
    server_process: Child,
    request_writer: ChildStdin,
    response_reader: BufReader<ChildStdout>,
}

impl LspProcessClient {
    async fn spawn() -> Self {
        let binary_path = engine_ai_lsp_binary_path();

        let mut server_process = Command::new(binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start engine-ai-lsp process for integration test");

        let request_writer = server_process
            .stdin
            .take()
            .expect("Failed to capture stdin for engine-ai-lsp process");

        let response_reader = BufReader::new(
            server_process
                .stdout
                .take()
                .expect("Failed to capture stdout for engine-ai-lsp process"),
        );

        Self {
            server_process,
            request_writer,
            response_reader,
        }
    }

    async fn send_request(&mut self, request_id: u64, method: &str, params: Value) -> Value {
        let request_payload = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });

        self.send_message(&request_payload).await;

        self.read_message().await
    }

    async fn send_notification(&mut self, method: &str, params: Value) {
        let notification_payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&notification_payload).await;
    }

    async fn send_message_batch(&mut self, messages: &[Value]) {
        let mut framed_batch_payload = Vec::new();

        for message in messages {
            let encoded_message = serde_json::to_vec(message).expect("Failed to encode JSON-RPC message while constructing framed batch");

            let encoded_header = format!("Content-Length: {}\r\n\r\n", encoded_message.len());

            framed_batch_payload.extend_from_slice(encoded_header.as_bytes());
            framed_batch_payload.extend_from_slice(&encoded_message);
        }

        let write_result = timeout(MESSAGE_TIMEOUT, async {
            self.request_writer.write_all(&framed_batch_payload).await?;
            self.request_writer.flush().await
        })
        .await
        .expect("Timed out while writing framed JSON-RPC batch payload");

        write_result.expect("Failed to write framed JSON-RPC batch payload to server stdin");
    }

    async fn send_message(&mut self, message: &Value) {
        let encoded_message = serde_json::to_vec(message).expect("Failed to encode JSON-RPC message for server request");

        let encoded_header = format!("Content-Length: {}\r\n\r\n", encoded_message.len());

        let write_result = timeout(MESSAGE_TIMEOUT, async {
            self.request_writer.write_all(encoded_header.as_bytes()).await?;
            self.request_writer.write_all(&encoded_message).await?;
            self.request_writer.flush().await
        })
        .await
        .expect("Timed out while writing JSON-RPC message payload");

        write_result.expect("Failed to write JSON-RPC message payload to server stdin");
    }

    async fn read_message(&mut self) -> Value {
        let mut content_length = None;

        loop {
            let mut header_line = String::new();

            let read_line_result = timeout(MESSAGE_TIMEOUT, self.response_reader.read_line(&mut header_line))
                .await
                .expect("Timed out while reading JSON-RPC response header");

            let bytes_read = read_line_result.expect("Failed to read JSON-RPC response header line");

            assert_ne!(bytes_read, 0, "Language server terminated before response was received");

            if header_line == "\r\n" {
                break;
            }

            if let Some(header_value) = header_line.strip_prefix("Content-Length:") {
                let parsed_length = header_value
                    .trim()
                    .parse::<usize>()
                    .expect("Failed to parse Content-Length header value as usize");

                content_length = Some(parsed_length);
            }
        }

        let message_length = content_length.expect("Missing Content-Length header in JSON-RPC response");
        let mut message_payload = vec![0_u8; message_length];

        let read_payload_result = timeout(MESSAGE_TIMEOUT, self.response_reader.read_exact(&mut message_payload))
            .await
            .expect("Timed out while reading JSON-RPC response payload");

        read_payload_result.expect("Failed to read JSON-RPC response payload bytes");

        serde_json::from_slice(&message_payload).expect("Failed to decode JSON-RPC response payload as JSON value")
    }

    async fn wait_for_exit(&mut self) {
        let wait_result = timeout(MESSAGE_TIMEOUT, self.server_process.wait())
            .await
            .expect("Timed out waiting for language server process to exit");

        let exit_status = wait_result.expect("Failed while waiting for language server process exit status");

        assert!(
            exit_status.success(),
            "Language server exited unsuccessfully with status {exit_status}"
        );
    }
}

fn engine_ai_lsp_binary_path() -> PathBuf {
    if let Ok(binary_path) = std::env::var("CARGO_BIN_EXE_engine-ai-lsp") {
        return PathBuf::from(binary_path);
    }

    if let Ok(binary_path) = std::env::var("CARGO_BIN_EXE_engine_ai_lsp") {
        return PathBuf::from(binary_path);
    }

    let current_test_binary_path = std::env::current_exe().expect("Failed to resolve current integration test binary path");

    let target_debug_directory = current_test_binary_path
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Failed to resolve target debug directory from integration test binary path");

    let binary_file_name = if cfg!(windows) { "engine-ai-lsp.exe" } else { "engine-ai-lsp" };

    let binary_path = target_debug_directory.join(binary_file_name);

    assert!(
        binary_path.exists(),
        "Failed to locate engine-ai-lsp binary at {:?}. Expected CARGO_BIN_EXE_engine-ai-lsp or CARGO_BIN_EXE_engine_ai_lsp.",
        binary_path
    );

    binary_path
}

impl Drop for LspProcessClient {
    fn drop(&mut self) {
        if self.server_process.try_wait().ok().flatten().is_none() {
            let _ = self.server_process.start_kill();
        }
    }
}

#[tokio::test]
async fn routes_lifecycle_completion_and_hover_requests_over_stdio() {
    let mut language_server_client = LspProcessClient::spawn().await;
    let document_uri = "file:///workspace/workflow.engine";

    let initialize_response = language_server_client
        .send_request(1, "initialize", json!({ "capabilities": {} }))
        .await;

    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 1);
    assert!(initialize_response["result"]["capabilities"]["completionProvider"].is_object());

    let initial_document_text = dsl! {
        input {
            first: string
        }

        agent helper {
            prompt: "Name: ${input.first}"
        }
    };

    language_server_client
        .send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": document_uri,
                    "text": initial_document_text,
                }
            }),
        )
        .await;

    let open_diagnostics_notification = language_server_client.read_message().await;

    assert_eq!(open_diagnostics_notification["jsonrpc"], "2.0");
    assert_eq!(open_diagnostics_notification["method"], "textDocument/publishDiagnostics");
    assert_eq!(open_diagnostics_notification["params"]["uri"], document_uri);
    assert!(open_diagnostics_notification["params"]["diagnostics"].is_array());

    let changed_document_text = dsl! {
        input {
            first: string
            last: string
        }

        agent helper {
            prompt: "Name: ${input.first}"
        }
    };

    language_server_client
        .send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": document_uri,
                },
                "contentChanges": [
                    {
                        "text": changed_document_text,
                    }
                ]
            }),
        )
        .await;

    let change_diagnostics_notification = language_server_client.read_message().await;

    assert_eq!(change_diagnostics_notification["jsonrpc"], "2.0");
    assert_eq!(change_diagnostics_notification["method"], "textDocument/publishDiagnostics");
    assert_eq!(change_diagnostics_notification["params"]["uri"], document_uri);

    let completion_response = language_server_client
        .send_request(
            2,
            "textDocument/completion",
            json!({
                "textDocument": {
                    "uri": document_uri,
                },
                "position": {
                    "line": 6,
                    "character": 20,
                }
            }),
        )
        .await;

    assert_eq!(completion_response["jsonrpc"], "2.0");
    assert_eq!(completion_response["id"], 2);
    assert!(completion_response["result"]["items"].is_array());

    let hover_response = language_server_client
        .send_request(
            3,
            "textDocument/hover",
            json!({
                "textDocument": {
                    "uri": document_uri,
                },
                "position": {
                    "line": 1,
                    "character": 12,
                }
            }),
        )
        .await;

    assert_eq!(hover_response["jsonrpc"], "2.0");
    assert_eq!(hover_response["id"], 3);
    assert!(hover_response.get("result").is_some());

    language_server_client
        .send_notification(
            "textDocument/didClose",
            json!({
                "textDocument": {
                    "uri": document_uri,
                }
            }),
        )
        .await;

    let close_diagnostics_notification = language_server_client.read_message().await;

    assert_eq!(close_diagnostics_notification["jsonrpc"], "2.0");
    assert_eq!(close_diagnostics_notification["method"], "textDocument/publishDiagnostics");
    assert_eq!(close_diagnostics_notification["params"]["uri"], document_uri);
    assert_eq!(close_diagnostics_notification["params"]["diagnostics"], json!([]));

    let shutdown_response = language_server_client.send_request(4, "shutdown", Value::Null).await;

    assert_eq!(shutdown_response["jsonrpc"], "2.0");
    assert_eq!(shutdown_response["id"], 4);
    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.send_notification("exit", Value::Null).await;
    language_server_client.wait_for_exit().await;
}

#[tokio::test]
async fn reads_multiple_framed_messages_from_single_input_batch() {
    let mut language_server_client = LspProcessClient::spawn().await;

    let initialize_request = json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "initialize",
        "params": {
            "capabilities": {}
        }
    });

    let shutdown_request = json!({
        "jsonrpc": "2.0",
        "id": 102,
        "method": "shutdown",
        "params": null
    });

    let exit_notification = json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": null
    });

    language_server_client
        .send_message_batch(&[initialize_request, shutdown_request, exit_notification])
        .await;

    let initialize_response = language_server_client.read_message().await;

    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 101);
    assert!(initialize_response["result"]["capabilities"].is_object());

    let shutdown_response = language_server_client.read_message().await;

    assert_eq!(shutdown_response["jsonrpc"], "2.0");
    assert_eq!(shutdown_response["id"], 102);
    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.wait_for_exit().await;
}
