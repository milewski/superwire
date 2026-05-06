#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_optional_chaining_for_present_nested_value() {
    let output = TestRunner::workflow(fixtures::OPTIONAL_CHAINING)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Generate data with a nullable nested object.")
                    .respond_json(json!({
                        "label": "test",
                        "details": { "score": 95, "tags": ["fast", "reliable"] },
                    }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute optional chaining workflow");

    assert_eq!(output.output, json!({ "label": "test", "score": 95, "tags": ["fast", "reliable"] }));
}
