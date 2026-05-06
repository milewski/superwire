#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn resolves_diamond_dependency_order() {
    let output = TestRunner::workflow(fixtures::DIAMOND_DEPENDENCY)
        .input(input!({ "topic": "performance" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Analyze performance from perspective A.")
                    .respond_string("analysis from A");

                model
                    .turn()
                    .expect_prompt("Analyze performance from perspective B.")
                    .respond_string("analysis from B");

                model
                    .turn()
                    .expect_prompt("A=analysis from A, B=analysis from B")
                    .respond_string("merged result");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute diamond dependency workflow");

    assert_eq!(output.output, json!({ "merged": "merged result" }));
}
