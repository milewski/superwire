#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_object_input_into_prompt() {
    let run_output = TestRunner::workflow(fixtures::INPUT_OBJECT)
        .input(input!({
            "product_name": "SuperWidget",
            "release_highlights": ["speed", "reliability"],
        }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().expect_prompt("Summarize SuperWidget highlights").respond_json(json!({
                    "summary": "Great product",
                    "key_points": ["fast", "reliable", "affordable"],
                }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute object input workflow");

    assert_eq!(
        run_output.output,
        json!({
            "summary": "Great product",
            "key_points": ["fast", "reliable", "affordable"],
        })
    );
}
