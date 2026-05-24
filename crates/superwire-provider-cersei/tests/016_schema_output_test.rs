#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_schema_output_and_field_access() {
    let output = TestRunner::workflow(fixtures::SCHEMA_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Generate a person profile.").respond_json(json!({
                    "value": {
                        "name": "Alice",
                        "age": 30,
                        "role": "engineer",
                    }
                }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute schema output workflow");

    assert_eq!(
        output.output,
        json!({
            "profile": { "name": "Alice", "age": 30, "role": "engineer" },
            "name": "Alice",
            "age": 30,
        })
    );
}
