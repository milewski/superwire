use crate::support::fixtures;
use crate::support::runner::TestRunner;

#[tokio::test]
async fn fails_before_execution_when_for_loop_agent_output_is_used_as_scalar() {
    let run_error = TestRunner::workflow(fixtures::FOR_LOOP_AGENT_OUTPUT_FIELD_REFERENCE)
        .run_expect_error()
        .await;

    let error_message = run_error.error.to_string();

    assert!(
        error_message.contains("invalid_reference_path") || error_message.contains("agent.random.user"),
        "{error_message}"
    );
}
