#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn agent_instruction_template_can_render_agent_context_expression() {
    let output = TestRunner::workflow(fixtures::AGENT_CONTEXT_EXPRESSION_PROMPT)
        .provider("openai", |provider| {
            provider.model("model-a", |model| {
                model.turn().respond_json(json!({ "result": "cat joke" }));
                model
                    .turn()
                    .expect_prompt("__superwire_cersei_context")
                    .respond_json(json!({ "result": "used context" }));
            });
        })
        .run()
        .await
        .expect("workflow should execute");

    assert_eq!(output.output, json!({ "result": "used context" }));
}
