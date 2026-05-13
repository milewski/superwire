#[macro_use]
mod support;

use serde_json::{json, Value};
use superwire_executor::runtime::ExecutorError;
use support::fixtures;
use support::runner::{TestRunner, ToolCall};

#[tokio::test]
async fn injects_finalize_tool_without_response_format() {
    let output = TestRunner::workflow(fixtures::AGENT_FINALIZE_TOOL)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        assert_eq!(messages.len(), 2);
                    })
                    .respond_json(json!({ "value": "hello" }));
            });
        })
        .run()
        .await
        .expect("workflow should finish through finalize");

    let request = output.provider_requests["openai"].first().expect("provider request should exist");
    let finalize_tool = request
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| tools.iter().find(|tool| tool.pointer("/function/name") == Some(&json!("finalize"))))
        .expect("finalize tool should be injected");

    assert!(request.get("response_format").is_none());
    assert_eq!(finalize_tool.pointer("/function/strict"), Some(&json!(true)));
    assert_eq!(finalize_tool.pointer("/function/parameters/type"), Some(&json!("object")));
    assert_eq!(output.output, json!({ "greeting": { "value": "hello" } }));
}

#[tokio::test]
async fn returns_finalize_schema_error_to_model_and_recovers() {
    let output = TestRunner::workflow(fixtures::AGENT_FINALIZE_TOOL)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .respond_tool_calls([ToolCall::new("finalize", json!({ "type": "success" }))]);

                model
                    .turn()
                    .with_messages(|messages| {
                        let tool_message = messages
                            .iter()
                            .find(|message| message.get("role") == Some(&json!("tool")))
                            .expect("invalid finalize call should be returned as tool content");
                        let content = tool_message
                            .get("content")
                            .and_then(Value::as_str)
                            .expect("tool content should be text");

                        assert!(content.contains("tool_argument_schema_mismatch"));
                        assert!(content.contains("Correct the arguments and call the tool again"));
                    })
                    .respond_json(json!({ "value": "recovered" }));
            });
        })
        .run()
        .await
        .expect("workflow should recover after invalid finalize arguments");

    assert_eq!(output.output, json!({ "greeting": { "value": "recovered" } }));
    assert_eq!(output.provider_requests["openai"].len(), 2);
}

#[tokio::test]
async fn nudges_model_when_it_stops_with_text() {
    let output = TestRunner::workflow(fixtures::AGENT_FINALIZE_TOOL)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().respond_text("hello without tool");

                model
                    .turn()
                    .with_messages(|messages| {
                        let nudge = messages.last().expect("nudge message should be appended");
                        let nudge_text = nudge
                            .get("content")
                            .and_then(Value::as_str)
                            .expect("nudge should have text content");

                        assert!(nudge_text.contains("must call the internal `finalize` tool"));
                    })
                    .respond_json(json!({ "value": "hello after nudge" }));
            });
        })
        .run()
        .await
        .expect("workflow should recover after nudge");

    assert_eq!(output.output, json!({ "greeting": { "value": "hello after nudge" } }));
    assert_eq!(output.provider_requests["openai"].len(), 2);
}

#[tokio::test]
async fn finalize_fail_returns_model_error() {
    let output = TestRunner::workflow(fixtures::AGENT_FINALIZE_TOOL)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().respond_tool_calls([ToolCall::new(
                    "finalize",
                    json!({
                        "type": "fail",
                        "reason": "missing required upstream data",
                    }),
                )]);
            });
        })
        .run_expect_error()
        .await;

    let ExecutorError::Model { message, .. } = output.error else {
        panic!("expected model error");
    };

    assert!(message.contains("missing required upstream data"));
}
