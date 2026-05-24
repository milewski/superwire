#![allow(dead_code, unused_macros)]

use std::{path::PathBuf, process::Stdio, time::Duration};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

macro_rules! dsl {
    ($($tokens:tt)*) => {{
        let source = stringify!($($tokens)*);
        let trimmed = source.trim_start_matches('\n').trim_end_matches(['\n', ' ']);
        trimmed.to_string()
    }};
}

const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LspProcessClient {
    server_process: Child,
    request_writer: ChildStdin,
    response_reader: BufReader<ChildStdout>,
}

impl LspProcessClient {
    #[must_use]
    pub fn spawn() -> Self {
        let binary_path = superwire_lsp_binary_path();

        let mut server_process = Command::new(binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start superwire-lsp process for integration test");

        let request_writer = server_process
            .stdin
            .take()
            .expect("Failed to capture stdin for superwire-lsp process");

        let response_reader = BufReader::new(
            server_process
                .stdout
                .take()
                .expect("Failed to capture stdout for superwire-lsp process"),
        );

        Self {
            server_process,
            request_writer,
            response_reader,
        }
    }

    pub async fn send_request(&mut self, request_id: u64, method: &str, params: Value) -> Value {
        let request_payload = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });

        self.send_message(&request_payload).await;

        self.read_message().await
    }

    pub async fn send_notification(&mut self, method: &str, params: Value) {
        let notification_payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&notification_payload).await;
    }

    pub async fn send_message_batch(&mut self, messages: &[Value]) {
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

    pub async fn read_message(&mut self) -> Value {
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

    pub async fn wait_for_exit(&mut self) {
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

impl Drop for LspProcessClient {
    fn drop(&mut self) {
        if self.server_process.try_wait().ok().flatten().is_none() {
            let _ = self.server_process.start_kill();
        }
    }
}

pub fn did_open_params(document_uri: &str, document_text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
            "languageId": "superwire",
            "version": 1,
            "text": document_text,
        }
    })
}

pub fn did_change_params(document_uri: &str, document_text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
            "version": 2,
        },
        "contentChanges": [
            {
                "text": document_text,
            }
        ]
    })
}

pub fn did_close_params(document_uri: &str) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
        }
    })
}

pub fn text_document_position_params(document_uri: &str, line: u64, character: u64) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
        },
        "position": {
            "line": line,
            "character": character,
        }
    })
}

pub fn text_document_params(document_uri: &str) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
        }
    })
}

pub fn text_document_formatting_params(document_uri: &str) -> Value {
    json!({
        "textDocument": {
            "uri": document_uri,
        },
        "options": {
            "tabSize": 4,
            "insertSpaces": true,
        }
    })
}

pub fn line_index_with_fragments(source_text: &str, fragments: &[&str]) -> u64 {
    source_text
        .lines()
        .position(|source_line| fragments.iter().all(|fragment| source_line.contains(fragment)))
        .and_then(|line_index| u64::try_from(line_index).ok())
        .expect("source should contain expected fragments")
}

pub fn character_index_for_fragment(source_text: &str, line_index: u64, fragment: &str) -> u64 {
    let line_index = usize::try_from(line_index).expect("line index should fit usize");
    let source_line = source_text.lines().nth(line_index).expect("source should include requested line");
    let byte_index = source_line.find(fragment).expect("source line should include expected fragment");

    u64::try_from(source_line[..byte_index].chars().count()).expect("character index should fit u64")
}

pub async fn assert_publish_diagnostics_for_uri(language_server_client: &mut LspProcessClient, document_uri: &str) -> Value {
    let diagnostics_notification = language_server_client.read_message().await;

    assert_eq!(diagnostics_notification["jsonrpc"], "2.0");
    assert_eq!(diagnostics_notification["method"], "textDocument/publishDiagnostics");
    assert_eq!(diagnostics_notification["params"]["uri"], document_uri);

    diagnostics_notification
}

fn superwire_lsp_binary_path() -> PathBuf {
    if let Ok(binary_path) = std::env::var("CARGO_BIN_EXE_superwire-lsp") {
        return PathBuf::from(binary_path);
    }

    if let Ok(binary_path) = std::env::var("CARGO_BIN_EXE_superwire_lsp") {
        return PathBuf::from(binary_path);
    }

    let current_test_binary_path = std::env::current_exe().expect("Failed to resolve current integration test binary path");
    let target_debug_directory = current_test_binary_path
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Failed to resolve target debug directory from integration test binary path");
    let binary_file_name = if cfg!(windows) { "superwire-lsp.exe" } else { "superwire-lsp" };
    let binary_path = target_debug_directory.join(binary_file_name);
    let binary_path_display = binary_path.display();

    assert!(
        binary_path.exists(),
        "Failed to locate superwire-lsp binary at {binary_path_display}. Expected CARGO_BIN_EXE_superwire-lsp or CARGO_BIN_EXE_superwire_lsp."
    );

    binary_path
}
