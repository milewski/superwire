#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_linear_chain_fixture_in_order() {
    let run_output = TestRunner::workflow(fixtures::LINEAR_CHAIN)
        .input(input!({ "topic": "testing" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().expect_prompt("testing").respond_string("first");
                model.turn().expect_prompt("first").respond_string("second");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute linear chain workflow");

    assert_eq!(run_output.output, json!({ "result": "second" }));
}
