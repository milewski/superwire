#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_fixture_with_inference_settings() {
    let output = TestRunner::workflow(fixtures::INFERENCE_SETTINGS)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Analyze the current release readiness.")
                    .respond_json(json!({ "value": "All systems go." }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute inference settings workflow");

    assert_eq!(output.output, json!({ "analysis": "All systems go." }));
}
