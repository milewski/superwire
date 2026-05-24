#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn compact_context_replaces_source_history_with_summary() {
    let output = TestRunner::workflow(fixtures::AGENT_CONTEXT_COMPACTION)
        .provider("openai", |provider| {
            provider.model("model-a", |model| {
                model.turn().respond_json(json!({ "value": "research" }));
                model.turn().respond_string("compact summary");
                model.turn().respond_json(json!({ "value": "summary" }));
            });
        })
        .run()
        .await
        .expect("workflow should execute");

    assert_eq!(output.output, json!({ "result": "summary" }));

    let provider_requests = output
        .provider_requests
        .get("openai")
        .expect("openai provider requests should be recorded");
    let summarize_messages = provider_requests[2]["messages"]
        .as_array()
        .expect("summarize request should include messages");
    let user_messages = summarize_messages
        .iter()
        .filter(|message| message["role"] == "user")
        .collect::<Vec<_>>();

    assert_eq!(user_messages.len(), 2);
    assert_eq!(user_messages[0]["content"], "compact summary");
    assert_eq!(user_messages[1]["content"], "Summarize the compacted context.");
}
