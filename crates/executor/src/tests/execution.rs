use super::fixtures;
use serde_json::json;

#[tokio::test]
async fn minimum_workflow_produces_output() {
    assert_eq!(
        execute!(fixtures::MINIMUM, output: "hello world").await,
        json!({ "greeting": "hello world" })
    );
}

#[tokio::test]
async fn string_output_workflow() {
    assert_eq!(
        execute!(fixtures::STRING_OUTPUT, output: "This is a summary.").await,
        json!({ "summary": "This is a summary." })
    );
}

#[tokio::test]
async fn object_output_workflow() {
    let output = execute! (
        fixtures::OBJECT_OUTPUT,
        output: {
            "name": "Alice",
            "age": 30,
            "role": "engineer"
        }
    )
    .await;

    assert_eq!(
        output,
        json!({
            "profile": {
                "name": "Alice",
                "age": 30,
                "role": "engineer"
            }
        })
    );
}

#[tokio::test]
async fn linear_chain_executes_in_order() {
    let output = execute!(
        fixtures::LINEAR_CHAIN,
        input: { "topic": "testing" },
        output: "first",
        output: "second",
    )
    .await;
    assert_eq!(output, json!({ "result": "second" }));
}

#[tokio::test]
async fn multiline_prompt_workflow() {
    let output = execute!(fixtures::MULTILINE_PROMPT, output: "Welcome!").await;
    assert_eq!(output, json!({ "message": "Welcome!" }));
}

#[tokio::test]
async fn inference_settings_workflow() {
    let output = execute!(fixtures::INFERENCE_SETTINGS, output: "All systems go.").await;
    assert_eq!(output, json!({ "analysis": "All systems go." }));
}
