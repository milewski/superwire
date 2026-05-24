#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_array_input_into_prompt() {
    let output = TestRunner::workflow(fixtures::INPUT_ARRAY)
        .input(json!({ "items": ["alpha", "beta"] }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Process these items")
                    .respond_json(json!({ "processed": ["item-a", "item-b"], "count": 2 }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute array input workflow");

    assert_eq!(output.output, json!({ "processed": ["item-a", "item-b"], "count": 2 }));
}
