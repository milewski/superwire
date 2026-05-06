#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_string_input_into_prompt() {
    let run_output = TestRunner::workflow(fixtures::INPUT_STRING)
        .input(input!({ "topic": "quantum computing" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write about quantum computing.")
                    .respond_string("written content");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute string input workflow");

    assert_eq!(run_output.output, json!({ "content": "written content" }));
}
