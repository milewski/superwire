#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_linear_chain_fixture_in_order() {
    let output = TestRunner::workflow(fixtures::LINEAR_CHAIN)
        .input(json!({ "topic": "testing" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("testing").respond_json(json!({ "value": "first" }));
                model.turn().expect_prompt("first").respond_json(json!({ "value": "second" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute linear chain workflow");

    assert_eq!(output.output, json!({ "result": "second" }));
}
