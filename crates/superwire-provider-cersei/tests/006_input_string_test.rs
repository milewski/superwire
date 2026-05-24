#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_string_input_into_prompt() {
    let output = TestRunner::workflow(fixtures::INPUT_STRING)
        .input(json!({ "topic": "quantum computing" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write about quantum computing.")
                    .respond_json(json!({ "value": "written content" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute string input workflow");

    assert_eq!(output.output, json!({ "content": "written content" }));
}
