mod harness;

use harness::{CliCommand, CommandOutputAssertions, TemporaryWorkspace};
use std::ffi::OsStr;

use serde_json::json;
use superwire_cli::{Application, ExitCode, ExitStatus};
use superwire_test_support::FakeMcpClientFactory;

#[test]
fn renders_runtime_input_mismatch_as_typed_invalid_input() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-run-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "input.wire",
        superwire_macros::workflow_source_template! {
            input {
                topic: string
            }

            output {
                topic: input.topic
            }
        },
    );
    let input_json = json!({ "topic": 123 }).to_string();
    let command_output = CliCommand::workflow_run([
        workflow_file_path.as_os_str(),
        std::ffi::OsStr::new("--input-json"),
        std::ffi::OsStr::new(&input_json),
        std::ffi::OsStr::new("--no-cache"),
    ])
    .environment_variable("SUPERWIRE_ERROR_FORMAT", "json")
    .output();

    command_output.assert_failure_code(2, "workflow run should classify runtime input mismatch as invalid input");

    let error_payload = command_output.stderr_json_value();

    assert_eq!(error_payload["code"], "invalid_input");
    assert_eq!(error_payload["details"]["code"], "invalid_input");
    assert_eq!(error_payload["details"]["stage"], "input");
    assert_eq!(error_payload["details"]["subject"]["type"], "workflow");
}

#[test]
fn renders_parse_failure_through_typed_diagnostic_pipeline() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-run-parse-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "parse-error.wire",
        superwire_macros::workflow_source_template! {
            invalid
        },
    );
    let command_output = CliCommand::workflow_run([workflow_file_path.as_os_str(), std::ffi::OsStr::new("--no-cache")])
        .environment_variable("SUPERWIRE_ERROR_FORMAT", "json")
        .output();

    command_output.assert_failure_code(2, "workflow run should classify parse failures as invalid input");

    let error_payload = command_output.stderr_json_value();

    assert_eq!(error_payload["code"], "invalid_input");
    assert_eq!(error_payload["details"]["code"], "invalid_workflow");
    assert_eq!(error_payload["details"]["stage"], "planning");
    assert_eq!(error_payload["details"]["subject"]["type"], "workflow");
}

#[test]
fn renders_build_failure_through_typed_diagnostic_pipeline() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-run-build-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "build-error.wire",
        superwire_macros::workflow_source_template! {
            input {
                first: string
            }

            input {
                second: string
            }

            output {
                first: input.first
            }
        },
    );
    let command_output = CliCommand::workflow_run([workflow_file_path.as_os_str(), std::ffi::OsStr::new("--no-cache")])
        .environment_variable("SUPERWIRE_ERROR_FORMAT", "json")
        .output();

    command_output.assert_failure_code(2, "workflow run should classify build failures as invalid input");

    let error_payload = command_output.stderr_json_value();

    assert_eq!(error_payload["code"], "invalid_input");
    assert_eq!(error_payload["details"]["code"], "invalid_workflow");
    assert_eq!(error_payload["details"]["stage"], "planning");
    assert_eq!(error_payload["details"]["subject"]["type"], "workflow");
}

#[test]
fn passes_intentional_mcp_factory_through_run_execution() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-run-mcp-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "mcp.wire",
        superwire_macros::workflow_source_template! {
            mcp local {
                endpoint: "http://example.invalid/mcp"
            }

            output {
                value: "ok"
            }
        },
    );
    let fake_mcp_client_factory = FakeMcpClientFactory::new().with_server("local", |_server_builder| {});
    let exit_status = Application::from_arguments([
        OsStr::new("superwire-cli"),
        OsStr::new("workflow"),
        OsStr::new("run"),
        workflow_file_path.as_os_str(),
        OsStr::new("--no-cache"),
    ])
    .run_with_mcp_client_factory(&fake_mcp_client_factory);

    assert_eq!(exit_status, ExitStatus::from_exit_code(ExitCode::Success));
}
