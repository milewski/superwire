#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_fixture_with_inference_settings() {
    let run_output = TestRunner::workflow(fixtures::INFERENCE_SETTINGS)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Analyze the current release readiness.")
                    .respond_string("All systems go.");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute inference settings workflow");

    assert_eq!(run_output.output, json!({ "analysis": "All systems go." }));
}
