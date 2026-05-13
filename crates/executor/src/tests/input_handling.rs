use super::fixtures;
use serde_json::json;

#[tokio::test]
async fn string_input_is_passed_to_agent_prompt() {
    let output = execute!(
        fixtures::INPUT_STRING,
        input: { "topic": "quantum computing" },
        output: { "value": "written content" },
    )
    .await;
    assert_eq!(output, json!({ "content": "written content" }));
}

#[tokio::test]
async fn object_input_with_arrays() {
    let output = execute!(
        fixtures::INPUT_OBJECT,
        input: {
            "product_name": "SuperWidget",
            "release_highlights": ["speed", "reliability"]
        },
        output: {
            "summary": "Great product",
            "key_points": ["fast", "reliable", "affordable"]
        },
    )
    .await;
    assert_eq!(
        output,
        json!({ "summary": "Great product", "key_points": ["fast", "reliable", "affordable"] })
    );
}

#[tokio::test]
async fn array_input_is_passed() {
    let output = execute!(
        fixtures::INPUT_ARRAY,
        input: { "items": ["alpha", "beta"] },
        output: { "processed": ["item-a", "item-b"], "count": 2 },
    )
    .await;
    assert_eq!(output, json!({ "processed": ["item-a", "item-b"], "count": 2 }));
}

#[tokio::test]
async fn rejects_input_type_mismatch() {
    let error = execute_error!(fixtures::INPUT_STRING, input: { "topic": 123 }).await;
    assert!(error.is_client_error());
    assert!(error.to_string().contains("declared `input` block"));
}

#[tokio::test]
async fn rejects_missing_required_input() {
    let error = execute_error!(fixtures::INPUT_STRING, input: {}).await;
    assert!(error.is_client_error());
    assert!(error.to_string().contains("declared `input` block"));
}

#[tokio::test]
async fn reports_missing_input_object_with_declared_input_context() {
    let error = execute_error!(fixtures::INPUT_STRING).await;

    assert!(error.is_client_error());
    assert!(error.to_string().contains("workflow declares an `input` block"));
}

#[tokio::test]
async fn rejects_unexpected_input_when_none_declared() {
    let error = execute_error!(fixtures::MINIMUM, input: { "unexpected": "value" }).await;
    assert!(error.is_client_error());
}
