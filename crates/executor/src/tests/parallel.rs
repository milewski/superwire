use super::fixtures;
use serde_json::json;

#[tokio::test]
async fn parallel_agents_execute_independently() {
    let output = execute!(
        fixtures::PARALLEL_AGENTS,
        input: { "product_name": "SuperWidget" },
        output: { "markdown": "# Release Notes" },
        output: { "posts": ["post1", "post2"] },
        output: { "subject": "Launch!", "body": "We launched!" },
    )
    .await;
    assert_eq!(
        output,
        json!({
            "changelog": "# Release Notes",
            "posts": ["post1", "post2"],
            "email_subject": "Launch!",
            "email_body": "We launched!"
        })
    );
}

#[tokio::test]
async fn diamond_dependency_resolves_correctly() {
    let output = execute!(
        fixtures::DIAMOND_DEPENDENCY,
        input: { "topic": "performance" },
        output: { "value": "analysis from A" },
        output: { "value": "analysis from B" },
        output: { "value": "merged result" },
    )
    .await;
    assert_eq!(output, json!({ "merged": "merged result" }));
}
