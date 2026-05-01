use super::fixtures;
use serde_json::json;

#[tokio::test]
async fn dynamic_values_are_computed_and_used() {
    let output = execute!(
        fixtures::DYNAMIC_VALUES,
        input: { "topic": "rust async" },
        output: { "summary": "done" },
    )
    .await;

    assert_eq!(
        output,
        json!({
            "topic": "rust async",
            "audience": "engineering",
            "max_bullets": 3,
            "prompt_prefix": "Write a concise update",
            "summary": "done"
        })
    );
}

#[tokio::test]
async fn multiple_dynamic_blocks_are_merged() {
    let output = execute!(
        fixtures::DYNAMIC_VALUES,
        input: { "topic": "testing" },
        output: { "summary": "ok" },
    )
    .await;

    assert_eq!(output["audience"], "engineering");
    assert_eq!(output["max_bullets"], 3);
    assert_eq!(output["prompt_prefix"], "Write a concise update");
}
