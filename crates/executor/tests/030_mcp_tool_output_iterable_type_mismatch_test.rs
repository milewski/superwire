#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn rejects_for_loop_over_non_iterable_mcp_tool_output_schema() {
    let run_error = TestRunner::workflow(fixtures::MCP_TOOL_OUTPUT_ITERABLE_TYPE_MISMATCH)
        .mcp("local", |mcp| {
            mcp.tool("fetch-numbers", |tool| {
                tool.input_schema(schema!({})).output_schema(schema!({ value: number }));
            });
        })
        .run_expect_error()
        .await;

    let error_message = run_error.error.to_string();
    let mcp_requests = &run_error.mcp_requests["local"];

    assert!(
        error_message.contains("iterable") || error_message.contains("array"),
        "{error_message}"
    );

    assert!(mcp_requests
        .iter()
        .any(|request| request.get("method") == Some(&json!("tools/list"))));

    assert!(!mcp_requests
        .iter()
        .any(|request| request.get("method") == Some(&json!("tools/call"))));
}

#[tokio::test]
async fn executes_for_loop_over_iterable_mcp_tool_output_schema() {
    let run_output = TestRunner::workflow(fixtures::MCP_TOOL_OUTPUT_ITERABLE_TYPE_MISMATCH)
        .mcp("local", |mcp| {
            mcp.tool("fetch-numbers", |tool| {
                tool.input_schema(schema!({}))
                    .output_schema(schema!({
                        "type": "array",
                        "items": { "type": "integer" },
                    }))
                    .respond_json(json!([1, 2, 3]));
            });
        })
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().expect_prompt("Write a note for 1.").respond_string("one");
                model.turn().expect_prompt("Write a note for 2.").respond_string("two");
                model.turn().expect_prompt("Write a note for 3.").respond_string("three");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute for-loop over MCP array output");

    let mcp_requests = &run_output.mcp_requests["local"];

    assert_eq!(run_output.output, json!({ "notes": ["one", "two", "three"] }));
    assert!(mcp_requests
        .iter()
        .any(|request| request.get("method") == Some(&json!("tools/list"))));
    assert!(mcp_requests
        .iter()
        .any(|request| request.get("method") == Some(&json!("tools/call"))));
    assert_eq!(run_output.provider_requests["openai"].len(), 3);
}
