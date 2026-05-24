mod harness;

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};

use harness::{CliCommand, CommandOutputAssertions, TemporaryWorkspace};
use serde_json::{json, Value};
use superwire_cli::{Application, ExitCode, ExitStatus};
use superwire_core::testing::{FakeMcpClientFactory, FakeMcpServerBuilder};

#[test]
fn writes_single_project_lock_for_multiple_workflows() {
    let fake_mcp_client_factory = fake_mcp_client_factory();
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let first_workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };

    let second_workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };

    let vars_path = temporary_workspace.write_json_file(
        ".wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": "http://example.invalid/mcp"
            }
        }),
    );

    let first_workflow_path = temporary_workspace.write_workflow("workflows/first.wire", &first_workflow_source);
    let second_workflow_path = temporary_workspace.write_workflow("workflows/second.wire", &second_workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let exit_status = run_workflow_lock_with_fake_mcp(
        [
            first_workflow_path.as_os_str(),
            second_workflow_path.as_os_str(),
            OsStr::new("--vars-file"),
            vars_path.as_os_str(),
            OsStr::new("--output"),
            output_lock_path.as_os_str(),
        ],
        &fake_mcp_client_factory,
    );

    assert_eq!(exit_status, ExitStatus::from_exit_code(ExitCode::Success));
    temporary_workspace.assert_file_exists(&output_lock_path, "project lock should be written");
    temporary_workspace.assert_file_missing(
        first_workflow_path.with_extension("wire.lock"),
        "per-workflow lock should not be written",
    );
    temporary_workspace.assert_file_missing(
        second_workflow_path.with_extension("wire.lock"),
        "per-workflow lock should not be written",
    );

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert_eq!(lock_json.pointer("/version"), Some(&json!(1)));
    assert_eq!(
        lock_json.pointer("/workflows/workflows~1first.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
    assert_eq!(
        lock_json.pointer("/workflows/workflows~1second.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );

    assert_eq!(
        fake_mcp_client_factory
            .requests("local")
            .iter()
            .filter(|request| request.method == "tools/list")
            .count(),
        2
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
fn subprocess_writes_single_project_lock_for_multiple_workflows() {
    let test_mcp_server = TestMcpHttpServer::spawn_with_mode(TestMcpServerMode::Standard);
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let first_workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };

    let second_workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };

    let vars_path = temporary_workspace.write_json_file(
        ".wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": test_mcp_server.endpoint()
            }
        }),
    );

    let first_workflow_path = temporary_workspace.write_workflow("workflows/first.wire", &first_workflow_source);
    let second_workflow_path = temporary_workspace.write_workflow("workflows/second.wire", &second_workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let command_output = CliCommand::workflow_lock([
        first_workflow_path.as_os_str(),
        second_workflow_path.as_os_str(),
        OsStr::new("--vars-file"),
        vars_path.as_os_str(),
        OsStr::new("--output"),
        output_lock_path.as_os_str(),
    ])
    .output();

    command_output.assert_success("workflow lock command should succeed");
    temporary_workspace.assert_file_exists(&output_lock_path, "project lock should be written");
    temporary_workspace.assert_file_missing(
        first_workflow_path.with_extension("wire.lock"),
        "per-workflow lock should not be written",
    );
    temporary_workspace.assert_file_missing(
        second_workflow_path.with_extension("wire.lock"),
        "per-workflow lock should not be written",
    );

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

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
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let first_workflow_source = superwire_dsl::workflow_source_template! {
        output {
            value: "first"
        }
    };

    let second_workflow_source = superwire_dsl::workflow_source_template! {
        output {
            value: "second"
        }
    };

    let workflow_directory_path = temporary_workspace.create_directory("workflows");
    temporary_workspace.write_workflow("workflows/first.wire", &first_workflow_source);
    temporary_workspace.write_workflow("workflows/nested/second.wire", &second_workflow_source);
    temporary_workspace.write_file("workflows/nested/notes.txt", "not a workflow");
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let command_output = CliCommand::workflow_lock([
        workflow_directory_path.as_os_str(),
        OsStr::new("--output"),
        output_lock_path.as_os_str(),
    ])
    .output();

    command_output.assert_success("workflow lock command should succeed");

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert!(lock_json.pointer("/workflows/workflows~1first.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1nested~1second.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1nested~1notes.txt").is_none());
}

#[test]
fn help_includes_project_lock_example() {
    let command_output = CliCommand::workflow_lock([OsStr::new("--help")]).output();

    command_output.assert_success("workflow lock help should succeed");
    command_output.assert_stdout_contains(
        "superwire-cli workflow lock .",
        "workflow lock help should include directory example",
    );
    command_output.assert_stdout_contains(
        "superwire-cli workflow lock workflows/*.wire --vars-file .wire.vars --output superwire.lock",
        "workflow lock help should include vars-file example",
    );
}

#[test]
fn writes_relative_workflow_keys_when_using_default_output_path() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        output {
            value: "ok"
        }
    };

    let workflow_path = temporary_workspace.write_workflow("workflows/absolute-input.wire", &workflow_source);
    let command_output = CliCommand::workflow_lock([workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    command_output.assert_success("workflow lock command should succeed");
    temporary_workspace.assert_file_exists(&output_lock_path, "project lock should be written");

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert!(lock_json.pointer("/workflows/workflows~1absolute-input.wire").is_some());
}

#[test]
fn reads_default_vars_file_next_to_custom_output_path() {
    let fake_mcp_client_factory = fake_mcp_client_factory();
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };
    let workflow_path = temporary_workspace.write_workflow("workflows/custom-output.wire", &workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("locks/superwire.lock");

    temporary_workspace.write_json_file(
        "locks/.wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": "http://example.invalid/mcp"
            }
        }),
    );

    let exit_status = run_workflow_lock_with_fake_mcp(
        [workflow_path.as_os_str(), OsStr::new("--output"), output_lock_path.as_os_str()],
        &fake_mcp_client_factory,
    );

    assert_eq!(exit_status, ExitStatus::from_exit_code(ExitCode::Success));
    temporary_workspace.assert_file_exists(&output_lock_path, "custom output lock should be written");
    temporary_workspace.assert_file_missing(
        temporary_workspace.root_directory.join(".wire.vars"),
        "default vars file should be resolved beside the lock output",
    );
    assert_eq!(fake_mcp_client_factory.requests("local").len(), 1);
}

#[test]
fn applies_vars_file_overrides_per_workflow_path() {
    let fake_mcp_client_factory = fake_mcp_client_factory();
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
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

        tool update_user_name from mcp.local.tool.update_user_name
    };
    let first_workflow_path = temporary_workspace.write_workflow("workflows/first.wire", &workflow_source);
    let second_workflow_path = temporary_workspace.write_workflow("workflows/second.wire", &workflow_source);
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
                        "mcp_endpoint": "http://example.invalid/first",
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
                        "mcp_endpoint": "http://example.invalid/second",
                        "models": {
                            "pro": "example"
                        }
                    }
                }
            }
        }),
    );
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let exit_status = run_workflow_lock_with_fake_mcp(
        [
            first_workflow_path.as_os_str(),
            second_workflow_path.as_os_str(),
            OsStr::new("--vars-file"),
            vars_path.as_os_str(),
            OsStr::new("--output"),
            output_lock_path.as_os_str(),
        ],
        &fake_mcp_client_factory,
    );

    assert_eq!(exit_status, ExitStatus::from_exit_code(ExitCode::Success));

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert_eq!(
        lock_json.pointer("/workflows/workflows~1first.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
    assert_eq!(
        lock_json.pointer("/workflows/workflows~1second.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
    assert_eq!(
        fake_mcp_client_factory
            .requests("local")
            .iter()
            .filter(|request| request.method == "tools/list")
            .count(),
        2
    );
}

#[test]
fn appends_new_workflows_when_lock_file_already_exists() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let first_workflow_source = superwire_dsl::workflow_source_template! {
        output {
            value: "first"
        }
    };
    let second_workflow_source = superwire_dsl::workflow_source_template! {
        output {
            value: "second"
        }
    };

    let first_workflow_path = temporary_workspace.write_workflow("workflows/first.wire", &first_workflow_source);
    let second_workflow_path = temporary_workspace.write_workflow("workflows/second.wire", &second_workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let first_command_output = CliCommand::workflow_lock([first_workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    first_command_output.assert_success("initial workflow lock command should succeed");

    let second_command_output = CliCommand::workflow_lock([second_workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    second_command_output.assert_success("second workflow lock command should succeed");

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert!(lock_json.pointer("/workflows/workflows~1first.wire").is_some());
    assert!(lock_json.pointer("/workflows/workflows~1second.wire").is_some());
}

#[test]
fn fails_when_mcp_server_requires_runtime_values_without_vars_context() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool fetch_task_data from mcp.local.tool.fetch_task_data
    };
    let workflow_path = temporary_workspace.write_workflow("workflows/dynamic-endpoint.wire", &workflow_source);
    let command_output = CliCommand::workflow_lock([workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    command_output.assert_failure("workflow lock command should fail without runtime context");
    command_output.assert_stderr_contains(
        "terminal is non-interactive",
        "workflow lock command should explain non-interactive mode",
    );
    command_output.assert_stderr_contains("secrets.mcp_endpoint", "workflow lock command should identify missing secret");
    command_output.assert_stderr_contains(".wire.vars", "workflow lock command should suggest vars file");
}

#[test]
fn fails_with_actionable_error_for_missing_prompted_values_in_non_interactive_mode() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        input {
            project_id: number
        }

        output {
            value: "ok"
        }
    };
    let workflow_path = temporary_workspace.write_workflow("workflows/missing-input.wire", &workflow_source);
    let command_output = CliCommand::workflow_lock([workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    command_output.assert_failure("workflow lock command should fail in non-interactive mode");
    command_output.assert_stderr_contains(
        "terminal is non-interactive",
        "workflow lock command should explain non-interactive mode",
    );
    command_output.assert_stderr_contains("input.project_id", "workflow lock command should identify missing input");
}

#[test]
fn reports_missing_nested_object_leaf_values_in_non_interactive_mode() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
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
    let workflow_path = temporary_workspace.write_workflow("workflows/missing-nested-secret.wire", &workflow_source);
    let command_output = CliCommand::workflow_lock([workflow_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    command_output.assert_failure("workflow lock command should fail in non-interactive mode");
    command_output.assert_stderr_contains(
        "terminal is non-interactive",
        "workflow lock command should explain non-interactive mode",
    );
    command_output.assert_stderr_contains(
        "secrets.models.flash",
        "workflow lock command should identify missing nested secret",
    );
    command_output.assert_stderr_not_contains(
        "secrets.models (json)",
        "workflow lock command should report missing object leaves instead of parent object",
    );
}

#[test]
fn generates_vars_file_from_workflow_directory() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        input {
            project_id: number
            task_group_id: number
        }

        secrets {
            endpoint: string
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
    let workflow_path = temporary_workspace.write_workflow("workflows/sample.wire", &workflow_source);
    let vars_path = temporary_workspace.root_directory.join(".wire.vars");
    let command_output = CliCommand::workflow_vars([workflow_path.as_os_str(), OsStr::new("--output"), vars_path.as_os_str()])
        .current_directory(&temporary_workspace.root_directory)
        .output();

    command_output.assert_success("workflow vars command should succeed");
    temporary_workspace.assert_file_exists(&vars_path, "workflow vars file should be written");

    let vars_json = temporary_workspace.read_json_file(&vars_path);

    assert_eq!(vars_json.pointer("/input/project_id"), Some(&json!(0)));
    assert_eq!(vars_json.pointer("/input/task_group_id"), Some(&json!(0)));
    assert_eq!(vars_json.pointer("/secrets/endpoint"), Some(&json!("")));
    assert_eq!(vars_json.pointer("/secrets/models/flash"), Some(&json!("")));
    assert_eq!(vars_json.pointer("/secrets/models/pro"), Some(&json!("")));
    assert_eq!(vars_json.pointer("/secrets/models/max"), Some(&json!("")));
}

#[test]
fn writes_partial_vars_file_even_when_some_workflows_fail_to_parse() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let valid_workflow_source = superwire_dsl::workflow_source_template! {
        input {
            project_id: number
        }

        output {
            value: "ok"
        }
    };
    let invalid_workflow_source = superwire_dsl::workflow_source_template! {
        input {
            project_id:
        }
    };

    let valid_workflow_path = temporary_workspace.write_workflow("workflows/valid.wire", &valid_workflow_source);
    let invalid_workflow_path = temporary_workspace.write_workflow("workflows/invalid.wire", &invalid_workflow_source);
    let vars_path = temporary_workspace.root_directory.join(".wire.vars");
    let command_output = CliCommand::workflow_vars([
        valid_workflow_path.as_os_str(),
        invalid_workflow_path.as_os_str(),
        OsStr::new("--output"),
        vars_path.as_os_str(),
    ])
    .current_directory(&temporary_workspace.root_directory)
    .output();

    command_output.assert_failure("workflow vars command should fail when one workflow cannot parse");
    temporary_workspace.assert_file_exists(&vars_path, "workflow vars file should still be written");
    command_output.assert_stderr_contains("generated", "workflow vars command should report generated partial file");
    command_output.assert_stderr_contains("partial values", "workflow vars command should report partial values");

    let vars_json = temporary_workspace.read_json_file(&vars_path);

    assert_eq!(vars_json.pointer("/input/project_id"), Some(&json!(0)));
}

struct TestMcpHttpServer {
    endpoint: String,
}

#[derive(Clone, Copy)]
enum TestMcpServerMode {
    Standard,
    RejectInitialize,
}

fn fake_mcp_client_factory() -> FakeMcpClientFactory {
    FakeMcpClientFactory::new().with_server("local", standard_mcp_server)
}

fn standard_mcp_server(server_builder: &mut FakeMcpServerBuilder) {
    server_builder.tool("update-user-name", |tool_builder| {
        tool_builder.description("Update user name").input_schema(json!({
            "type": "object",
            "properties": {
                "user_name": { "type": "string" }
            },
            "required": ["user_name"]
        }));
    });
}

fn run_workflow_lock_with_fake_mcp(
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    mcp_client_factory: &FakeMcpClientFactory,
) -> ExitStatus {
    let mut application_arguments = vec!["superwire-cli".into(), "workflow".into(), "lock".into()];

    application_arguments.extend(arguments.into_iter().map(|argument| argument.as_ref().to_os_string()));

    Application::from_arguments(application_arguments).run_with_mcp_client_factory(mcp_client_factory)
}

impl TestMcpHttpServer {
    fn spawn_with_mode(server_mode: TestMcpServerMode) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));

        std::thread::spawn(move || {
            for incoming_stream in listener.incoming().take(24) {
                let stream = incoming_stream.expect("test MCP stream should open");
                handle_mcp_request(stream, server_mode);
            }
        });

        Self { endpoint }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }
}

fn handle_mcp_request(mut stream: std::net::TcpStream, server_mode: TestMcpServerMode) {
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
    let request_method = request.get("method").and_then(Value::as_str);
    let response = if let Some(response_body) = response_for_method(request_method) {
        let response_body = response_body.to_string();

        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        )
    } else if matches!(server_mode, TestMcpServerMode::RejectInitialize)
        && (request_method == Some("initialize") || request_method == Some("notifications/initialized"))
    {
        "HTTP/1.1 406 Not Acceptable\r\ncontent-length: 0\r\n\r\n".to_string()
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

#[test]
fn continues_lock_discovery_when_mcp_server_rejects_initialize_endpoints() {
    let test_mcp_server = TestMcpHttpServer::spawn_with_mode(TestMcpServerMode::RejectInitialize);
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-lock-tests");
    let workflow_source = superwire_dsl::workflow_source_template! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };

    let workflow_path = temporary_workspace.write_workflow("workflows/reject-init.wire", &workflow_source);
    let output_lock_path = temporary_workspace.root_directory.join("superwire.lock");

    let vars_path = temporary_workspace.write_json_file(
        ".wire.vars",
        &json!({
            "secrets": {
                "mcp_endpoint": test_mcp_server.endpoint()
            }
        }),
    );

    let command_output = CliCommand::workflow_lock([
        workflow_path.as_os_str(),
        OsStr::new("--vars-file"),
        vars_path.as_os_str(),
        OsStr::new("--output"),
        output_lock_path.as_os_str(),
    ])
    .output();

    command_output.assert_success("workflow lock command should continue when initialize endpoints return 406");

    let lock_json = temporary_workspace.read_json_file(&output_lock_path);

    assert_eq!(
        lock_json.pointer("/workflows/workflows~1reject-init.wire/servers/local/tools/update-user-name/name"),
        Some(&json!("update-user-name"))
    );
}
