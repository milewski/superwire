use super::fixtures;
use serde_json::json;

#[tokio::test]
async fn secrets_are_accepted() {
    let output = execute_secrets!(
        fixtures::SECRETS,
        input: null,
        secrets: { "api_key": "sk-test-123" },
        output: "hello",
    )
    .await;
    assert_eq!(output, json!({ "greeting": "hello" }));
}

#[tokio::test]
async fn rejects_missing_required_secret() {
    let error = execute_secrets_error!(
        fixtures::SECRETS,
        input: null,
        secrets: null,
    )
    .await;
    assert!(error.is_client_error());
}
