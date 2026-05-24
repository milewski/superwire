#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn direct_agent_context_shares_source_history() {
    let output = TestRunner::workflow(fixtures::AGENT_CONTEXT_SHARING)
        .provider("openai", |provider| {
            provider.model("model-a", |model| {
                model.turn().respond_json(json!({ "value": "research" }));
                model.turn().respond_json(json!({ "value": "continued" }));
            });
        })
        .run()
        .await
        .expect("workflow should execute");

    assert_eq!(output.output, json!({ "result": "continued" }));

    let provider_requests = output
        .provider_requests
        .get("openai")
        .expect("openai provider requests should be recorded");
    let continue_messages = provider_requests[1]["messages"]
        .as_array()
        .expect("continue request should include messages");
    let user_messages = continue_messages
        .iter()
        .filter(|message| message["role"] == "user")
        .collect::<Vec<_>>();

    assert_eq!(user_messages.len(), 2);
    assert_eq!(user_messages[0]["content"], "Research the migration plan.");
    assert_eq!(user_messages[1]["content"], "Continue from the prior context with a new tool set.");
    assert!(continue_messages
        .iter()
        .any(|message| message["role"] == "assistant" && message["content"] == json!("{\"value\":\"research\"}")));
}
