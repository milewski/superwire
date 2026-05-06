#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn sends_multiline_prompt_to_provider() {
    let run_output = TestRunner::workflow(fixtures::MULTILINE_PROMPT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("You are a friendly assistant.")
                    .expect_prompt("Write a short welcome message.")
                    .expect_prompt("Keep it to one sentence.")
                    .respond_string("Welcome!");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute multiline prompt workflow");

    assert_eq!(run_output.output, json!({ "message": "Welcome!" }));
}
