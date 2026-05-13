use crate::support::fixtures;
use crate::support::runner::TestRunner;

#[tokio::test]
async fn fails_when_secret_reference_is_used_inside_instruction_template() {
    let run_error = TestRunner::workflow(fixtures::SECRETS_IN_INSTRUCTION_TEMPLATE)
        .secrets(serde_json::json!({ "api_key": "sk-test-123" }))
        .run_expect_error()
        .await;

    let error_message = run_error.error.to_string();

    assert!(
        error_message.contains("secret_reference_in_llm_context") || error_message.contains("Secret reference"),
        "{error_message}"
    );
}
