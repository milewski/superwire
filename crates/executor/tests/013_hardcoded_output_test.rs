#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn combines_hardcoded_and_agent_output_values() {
    let output = TestRunner::workflow(fixtures::HARDCODED_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Say hello.").respond_json(json!({ "value": "agent value" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute hardcoded output workflow");

    assert_eq!(
        output.output,
        json!({
            "hardcoded_string": "fixed-value",
            "hardcoded_number": 42,
            "hardcoded_boolean": true,
            "agent_value": "agent value",
        })
    );
}
