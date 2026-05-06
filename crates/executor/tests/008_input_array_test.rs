#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_array_input_into_prompt() {
    let run_output = TestRunner::workflow(fixtures::INPUT_ARRAY)
        .input(input!({ "items": ["alpha", "beta"] }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Process these items")
                    .respond_json(json!({ "processed": ["item-a", "item-b"], "count": 2 }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute array input workflow");

    assert_eq!(run_output.output, json!({ "processed": ["item-a", "item-b"], "count": 2 }));
}
