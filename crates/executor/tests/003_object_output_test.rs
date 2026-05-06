#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn asserts_response_format_for_structured_json() {
    let run_output = TestRunner::workflow(fixtures::OBJECT_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
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
        run_output.output,
        json!({
            "profile": {
                "name": "Ada",
                "age": 37,
                "role": "engineer",
            }
        })
    );
}
