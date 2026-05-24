#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_structured_json_output() {
    let output = TestRunner::workflow(fixtures::OBJECT_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Generate a user profile").respond_json(json!({
                    "name": "Ada",
                    "age": 37,
                    "role": "engineer",
                }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute structured output workflow");

    assert_eq!(
        output.output,
        json!({
            "profile": {
                "name": "Ada",
                "age": 37,
                "role": "engineer",
            }
        })
    );
}
