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
            "summary": "done"
        })
    );
}
