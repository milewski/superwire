mod harness;

use harness::{CliCommand, CommandOutputAssertions, TemporaryWorkspace};

#[test]
fn validates_workflow_file_when_check_command_succeeds() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-check-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "valid.wire",
        &superwire_core::workflow_source_template! {
            output {
                ok: true
            }
        },
    );

    let command_output = CliCommand::workflow_check(workflow_file_path.as_path()).output();

    command_output.assert_success("workflow check command should succeed");
    command_output.assert_stdout_contains("workflow is valid", "workflow check command should report valid workflow");
}

#[test]
fn rejects_workflow_file_with_invalid_reference_types() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-workflow-check-tests");
    let workflow_file_path = temporary_workspace.write_workflow(
        "invalid.wire",
        &superwire_core::workflow_source_template! {
            input {
                title: string
            }

            output {
                summary: input.missing
            }
        },
    );

    let command_output = CliCommand::workflow_check(workflow_file_path.as_path()).output();

    command_output.assert_failure_code(2, "workflow check command should fail");
    command_output.assert_stderr_contains("missing", "workflow check command should include validation details");
}
