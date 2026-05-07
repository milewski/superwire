use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

macro_rules! workflow_template {
    ($($workflow_tokens:tt)*) => {{
        stringify!($($workflow_tokens)*)
    }};
}

#[test]
fn writes_single_project_lock_for_multiple_workflows() {
    let test_mcp_server = TestMcpHttpServer::spawn();
    let temporary_workspace = TemporaryWorkspace::new();
    let first_workflow_source = workflow_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update-user-name
    };

    let second_workflow_source = workflow_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update-user-name
    };

    let vars_path = temporary_workspace.write_json_file(
        ".wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": test_mcp_server.endpoint()
            }
        }),
    );

    let first_workflow_path = temporary_workspace.write_file("workflows/first.wire", first_workflow_source);
    let second_workflow_path = temporary_workspace.write_file("workflows/second.wire", second_workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let command_output = run_workflow_lock_command(&[
        first_workflow_path.as_os_str(),
        second_workflow_path.as_os_str(),
        std::ffi::OsStr::new("--vars-file"),
        vars_path.as_os_str(),
        std::ffi::OsStr::new("--output"),
        output_lock_path.as_os_str(),
    ]);

    assert!(command_output.status.success(), "workflow lock command should succeed");
    assert!(output_lock_path.exists(), "project lock should be written");
    assert!(
        !first_workflow_path.with_extension("wire.lock").exists(),
        "per-workflow lock should not be written"
    );
    assert!(
        !second_workflow_path.with_extension("wire.lock").exists(),
        "per-workflow lock should not be written"
    );

    let lock_json: Value =
        serde_json::from_str(&fs::read_to_string(output_lock_path).expect("lock should read")).expect("lock should be valid json");

    assert_eq!(lock_json.pointer("/version"), Some(&json!(1)));
    assert_eq!(
        lock_json.pointer("/workflows/workflows~1first.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );

    assert_eq!(
        lock_json.pointer("/workflows/workflows~1second.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );

    assert!(lock_json.pointer("/workflows/workflows~1first.wire/resolution_context").is_none());

    assert!(
        lock_json
            .pointer("/workflows/workflows~1first.wire/hash")
            .and_then(Value::as_str)
            .is_some_and(|hash| !hash.is_empty()),
        "workflow lock entry should include integrity hash"
    );
}

#[test]
fn recursively_locks_workflows_from_directory_target() {
    let temporary_workspace = TemporaryWorkspace::new();
    let first_workflow_source = workflow_template! {
        output {
            value: "first"
        }
    };

    let second_workflow_source = workflow_template! {
        output {
            value: "second"
        }
    };

    let workflow_directory_path = temporary_workspace.create_directory("workflows");
    temporary_workspace.write_file("workflows/first.wire", first_workflow_source);
    temporary_workspace.write_file("workflows/nested/second.wire", second_workflow_source);
    temporary_workspace.write_file("workflows/nested/notes.txt", "not a workflow");
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let command_output = run_workflow_lock_command(&[
        workflow_directory_path.as_os_str(),
        std::ffi::OsStr::new("--output"),
        output_lock_path.as_os_str(),
    ]);

    assert!(command_output.status.success(), "workflow lock command should succeed");

    let lock_json: Value =
        serde_json::from_str(&fs::read_to_string(output_lock_path).expect("lock should read")).expect("lock should be valid json");

    assert!(lock_json.pointer("/workflows/workflows~1first.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1nested~1second.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1nested~1notes.txt").is_none());
}

#[test]
fn help_includes_project_lock_example() {
    let command_output = run_workflow_lock_command(&[std::ffi::OsStr::new("--help")]);
    let standard_output = String::from_utf8_lossy(&command_output.stdout);

    assert!(command_output.status.success(), "workflow lock help should succeed");
    assert!(standard_output.contains("superwire-cli workflow lock ."));
    assert!(standard_output.contains("superwire-cli workflow lock workflows/*.wire --vars-file .wire.vars --output superwire.lock"));
}

#[test]
fn writes_relative_workflow_keys_when_using_default_output_path() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        output {
            value: "ok"
        }
    };

    let workflow_path = temporary_workspace.write_file("workflows/absolute-input.wire", workflow_source);
    let command_output =
        run_workflow_lock_command_with_current_directory(&[workflow_path.as_os_str()], &temporary_workspace.root_directory);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    assert!(command_output.status.success(), "workflow lock command should succeed");
    assert!(output_lock_path.exists(), "project lock should be written");

    let lock_json: Value =
        serde_json::from_str(&fs::read_to_string(output_lock_path).expect("lock should read")).expect("lock should be valid json");

    assert!(lock_json.pointer("/workflows/workflows~1absolute-input.wire").is_some());
}

#[test]
fn reads_default_vars_file_next_to_custom_output_path() {
    let test_mcp_server = TestMcpHttpServer::spawn();
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update-user-name
    };
    let workflow_path = temporary_workspace.write_file("workflows/custom-output.wire", workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("locks/superwire.lock");

    temporary_workspace.write_json_file(
        "locks/.wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": test_mcp_server.endpoint()
            }
        }),
    );

    let command_output = run_workflow_lock_command_with_current_directory(
        &[
            workflow_path.as_os_str(),
            std::ffi::OsStr::new("--output"),
            output_lock_path.as_os_str(),
        ],
        &temporary_workspace.root_directory,
    );

    assert!(command_output.status.success(), "workflow lock command should succeed");
    assert!(output_lock_path.exists(), "custom output lock should be written");
    assert!(
        !temporary_workspace.root_directory.join(".wire.vars").exists(),
        "default vars file should be resolved beside the lock output"
    );
}

#[test]
fn applies_vars_file_overrides_per_workflow_path() {
    let test_mcp_server = TestMcpHttpServer::spawn();
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        input {
            project_id: number
            task_id: number
        }

        secrets {
            mcp_endpoint: string
            models: {
                flash: string
                max: string
                pro: string
            }
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update-user-name
    };
    let first_workflow_path = temporary_workspace.write_file("workflows/first.wire", workflow_source);
    let second_workflow_path = temporary_workspace.write_file("workflows/second.wire", workflow_source);
    let vars_path = temporary_workspace.write_json_file(
        ".wire.vars",
        &json!({
            "input": {
                "project_id": 14
            },
            "secrets": {
                "models": {
                    "flash": "example",
                    "max": "example"
                }
            },
            "overrides": {
                "workflows/first.wire": {
                    "input": {
                        "task_id": 109
                    },
                    "secrets": {
                        "mcp_endpoint": test_mcp_server.endpoint(),
                        "models": {
                            "pro": "example"
                        }
                    }
                },
                "workflows/second.wire": {
                    "input": {
                        "task_id": 110
                    },
                    "secrets": {
                        "mcp_endpoint": test_mcp_server.endpoint(),
                        "models": {
                            "pro": "example"
                        }
                    }
                }
            }
        }),
    );
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let command_output = run_workflow_lock_command_with_current_directory(
        &[
            first_workflow_path.as_os_str(),
            second_workflow_path.as_os_str(),
            std::ffi::OsStr::new("--vars-file"),
            vars_path.as_os_str(),
            std::ffi::OsStr::new("--output"),
            output_lock_path.as_os_str(),
        ],
        &temporary_workspace.root_directory,
    );
    let standard_error = String::from_utf8_lossy(&command_output.stderr);

    assert!(
        command_output.status.success(),
        "workflow lock command should succeed: {standard_error}"
    );

    let lock_json: Value =
        serde_json::from_str(&fs::read_to_string(output_lock_path).expect("lock should read")).expect("lock should be valid json");

    assert_eq!(
        lock_json.pointer("/workflows/workflows~1first.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
    assert_eq!(
        lock_json.pointer("/workflows/workflows~1second.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
}

#[test]
fn appends_new_workflows_when_lock_file_already_exists() {
    let temporary_workspace = TemporaryWorkspace::new();
    let first_workflow_source = workflow_template! {
        output {
            value: "first"
        }
    };
    let second_workflow_source = workflow_template! {
        output {
            value: "second"
        }
    };

    let first_workflow_path = temporary_workspace.write_file("workflows/first.wire", first_workflow_source);
    let second_workflow_path = temporary_workspace.write_file("workflows/second.wire", second_workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let first_command_output =
        run_workflow_lock_command_with_current_directory(&[first_workflow_path.as_os_str()], &temporary_workspace.root_directory);

    assert!(
        first_command_output.status.success(),
        "initial workflow lock command should succeed"
    );

    let second_command_output =
        run_workflow_lock_command_with_current_directory(&[second_workflow_path.as_os_str()], &temporary_workspace.root_directory);

    assert!(
        second_command_output.status.success(),
        "second workflow lock command should succeed"
    );

    let lock_json: Value =
        serde_json::from_str(&fs::read_to_string(output_lock_path).expect("lock should read")).expect("lock should be valid json");

    assert!(lock_json.pointer("/workflows/workflows~1first.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1second.wire").is_some());
}

#[test]
fn fails_when_mcp_server_requires_runtime_values_without_vars_context() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool fetch_task_data from mcp.local.tool.fetch_task_data
    };
    let workflow_path = temporary_workspace.write_file("workflows/dynamic-endpoint.wire", workflow_source);
    let command_output =
        run_workflow_lock_command_with_current_directory(&[workflow_path.as_os_str()], &temporary_workspace.root_directory);
    let standard_error = String::from_utf8_lossy(&command_output.stderr);

    assert!(
        !command_output.status.success(),
        "workflow lock command should fail without runtime context"
    );
    assert!(standard_error.contains("terminal is non-interactive"));
    assert!(standard_error.contains("secrets.mcp_endpoint"));
    assert!(standard_error.contains(".wire.vars"));
}

#[test]
fn fails_with_actionable_error_for_missing_prompted_values_in_non_interactive_mode() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        input {
            project_id: number
        }

        output {
            value: "ok"
        }
    };
    let workflow_path = temporary_workspace.write_file("workflows/missing-input.wire", workflow_source);
    let command_output =
        run_workflow_lock_command_with_current_directory(&[workflow_path.as_os_str()], &temporary_workspace.root_directory);
    let standard_error = String::from_utf8_lossy(&command_output.stderr);

    assert!(
        !command_output.status.success(),
        "workflow lock command should fail in non-interactive mode"
    );
    assert!(standard_error.contains("terminal is non-interactive"));
    assert!(standard_error.contains("input.project_id"));
}

#[test]
fn reports_missing_nested_object_leaf_values_in_non_interactive_mode() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        secrets {
            models: {
                flash: string
                pro: string
                max: string
            }
        }

        output {
            value: "ok"
        }
    };
    let workflow_path = temporary_workspace.write_file("workflows/missing-nested-secret.wire", workflow_source);
    let command_output =
        run_workflow_lock_command_with_current_directory(&[workflow_path.as_os_str()], &temporary_workspace.root_directory);
    let standard_error = String::from_utf8_lossy(&command_output.stderr);

    assert!(
        !command_output.status.success(),
        "workflow lock command should fail in non-interactive mode"
    );
    assert!(standard_error.contains("terminal is non-interactive"));
    assert!(standard_error.contains("secrets.models.flash"));
    assert!(!standard_error.contains("secrets.models (json)"));
}

fn run_workflow_lock_command(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(cli_binary_path())
        .arg("workflow")
        .arg("lock")
        .args(arguments)
        .output()
        .expect("workflow lock command should run")
}

fn run_workflow_lock_command_with_current_directory(arguments: &[&std::ffi::OsStr], current_directory: &Path) -> Output {
    Command::new(cli_binary_path())
        .arg("workflow")
        .arg("lock")
        .args(arguments)
        .current_dir(current_directory)
        .output()
        .expect("workflow lock command should run")
}

fn cli_binary_path() -> PathBuf {
    if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire-cli") {
        return PathBuf::from(configured_binary_path);
    }

    if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire_cli") {
        return PathBuf::from(configured_binary_path);
    }

    let current_executable_path = std::env::current_exe()
        .unwrap_or_else(|current_executable_error| panic!("failed to resolve current test executable path: {current_executable_error}"));
    let target_profile_directory = current_executable_path.parent().and_then(Path::parent).unwrap_or_else(|| {
        panic!(
            "failed to derive target profile directory from {}",
            current_executable_path.display()
        )
    });
    let executable_file_name = format!("superwire-cli{}", std::env::consts::EXE_SUFFIX);
    let inferred_binary_path = target_profile_directory.join(executable_file_name);

    if inferred_binary_path.exists() {
        return inferred_binary_path;
    }

    panic!(
        "failed to locate superwire-cli binary; looked for compile-time cargo bin vars and {}",
        inferred_binary_path.display()
    );
}

struct TemporaryWorkspace {
    root_directory: PathBuf,
}

impl TemporaryWorkspace {
    fn new() -> Self {
        let unique_suffix = unique_suffix();
        let root_directory = std::env::temp_dir().join(format!("superwire-workflow-lock-tests-{unique_suffix}"));

        fs::create_dir_all(&root_directory).expect("temporary root directory should be created");

        Self { root_directory }
    }

    fn write_file(&self, relative_path: &str, contents: &str) -> PathBuf {
        let absolute_path = self.root_directory.join(relative_path);

        if let Some(parent_directory) = absolute_path.parent() {
            fs::create_dir_all(parent_directory).expect("parent directory should be created");
        }

        fs::write(&absolute_path, contents).expect("temporary file should be written");

        absolute_path
    }

    fn create_directory(&self, relative_path: &str) -> PathBuf {
        let absolute_path = self.root_directory.join(relative_path);

        fs::create_dir_all(&absolute_path).expect("temporary directory should be created");

        absolute_path
    }

    fn write_json_file(&self, relative_path: &str, value: &Value) -> PathBuf {
        let contents = serde_json::to_string_pretty(value).expect("json should serialize");

        self.write_file(relative_path, &contents)
    }
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_directory);
    }
}

struct TestMcpHttpServer {
    endpoint: String,
}

impl TestMcpHttpServer {
    fn spawn() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));

        std::thread::spawn(move || {
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

fn handle_mcp_request(mut stream: std::net::TcpStream) {
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
        Some("tools/list") => Some(json!({
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
        Some("resources/list") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "resources": []
            }
        })),
        Some("prompts/list") => Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "prompts": []
            }
        })),
        _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
    }
}

fn unique_suffix() -> String {
    static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    let process_identifier = std::process::id();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{timestamp_millis}-{process_identifier}-{counter}")
}
