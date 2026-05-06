#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_string_output_fixture() {
    let output = TestRunner::workflow(fixtures::STRING_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write a one-sentence project summary.")
                    .respond_string("This is a summary.");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute string output workflow");

    assert_eq!(output.output, json!({ "summary": "This is a summary." }));
}
