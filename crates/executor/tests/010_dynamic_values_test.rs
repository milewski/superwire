#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn injects_dynamic_values_into_prompt_and_output() {
    let run_output = TestRunner::workflow(fixtures::DYNAMIC_VALUES)
        .input(input!({ "topic": "rust async" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write a concise update for engineering about rust async. Limit to 3 bullets.")
                    .respond_json(json!({ "summary": "done" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute dynamic values workflow");

    assert_eq!(
        run_output.output,
        json!({
            "topic": "rust async",
            "audience": "engineering",
            "max_bullets": 3,
            "prompt_prefix": "Write a concise update",
            "summary": "done",
        })
    );
}
