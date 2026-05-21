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
    let standard_output = command_output.stdout_text();

    command_output.assert_success("workflow check command should succeed");
    assert!(standard_output.contains("workflow is valid"));
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
    let standard_error = command_output.stderr_text();

    command_output.assert_failure_code(2, "workflow check command should fail");
    assert!(
        standard_error.contains("missing") || standard_error.contains("unknown"),
        "expected validation error details in stderr, received: {standard_error}"
    );
}
