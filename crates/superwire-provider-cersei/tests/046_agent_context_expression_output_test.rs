#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn workflow_output_can_include_agent_context_expression() {
    let output = TestRunner::workflow(fixtures::AGENT_CONTEXT_EXPRESSION_OUTPUT)
        .provider("openai", |provider| {
            provider.model("model-a", |model| {
                model.turn().respond_json(json!({ "result": "cat joke" }));
            });
        })
        .run()
        .await
        .expect("workflow should execute");

    assert_eq!(output.output["context_value"]["__superwire_cersei_context"], true);
    assert!(output.output["context_text"]
        .as_str()
        .is_some_and(|context_text| context_text.contains("__superwire_cersei_context")));
}
