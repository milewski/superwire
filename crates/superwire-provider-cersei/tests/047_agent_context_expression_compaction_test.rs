#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn workflow_output_can_include_compacted_agent_context_expression() {
    let output = TestRunner::workflow(fixtures::AGENT_CONTEXT_EXPRESSION_COMPACTION)
        .provider("openai", |provider| {
            provider.model("model-a", |model| {
                model.turn().respond_json(json!({ "result": "cat joke" }));
                model.turn().respond_string("compact joke context");
            });
        })
        .run()
        .await
        .expect("workflow should execute");

    let messages = output.output["compacted"]["messages"]
        .as_array()
        .expect("compacted context should include messages");

    assert_eq!(output.output["compacted"]["__superwire_cersei_context"], true);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "compact joke context");
}
