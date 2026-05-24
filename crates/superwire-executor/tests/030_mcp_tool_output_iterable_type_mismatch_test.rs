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
                tool.input_schema(schema! {}).output_schema(schema! { values: u64 });
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

    assert!(mcp_requests.iter().any(|request| request.method == "tools/list"));

    assert!(!mcp_requests.iter().any(|request| request.method == "tools/call"));
}

#[tokio::test]
async fn executes_for_loop_over_iterable_mcp_tool_output_schema() {
    let output = TestRunner::workflow(fixtures::MCP_TOOL_OUTPUT_ITERABLE_TYPE_MISMATCH)
        .mcp("local", |mcp| {
            mcp.tool("fetch-numbers", |tool| {
                tool.input_schema(schema! {})
                    .output_schema(schema! { values: Vec<u64> })
                    .respond_json(json!({ "values": [1, 2, 3] }));
            });
        })
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write a note for 1.")
                    .respond_json(json!({ "value": "one" }));
                model
                    .turn()
                    .expect_prompt("Write a note for 2.")
                    .respond_json(json!({ "value": "two" }));
                model
                    .turn()
                    .expect_prompt("Write a note for 3.")
                    .respond_json(json!({ "value": "three" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute for-loop over MCP array output");

    let mcp_requests = &output.mcp_requests["local"];

    assert_eq!(
        output.output,
        json!({ "notes": [{ "value": "one" }, { "value": "two" }, { "value": "three" }] })
    );
    assert!(mcp_requests.iter().any(|request| request.method == "tools/list"));

    assert!(mcp_requests.iter().any(|request| request.method == "tools/call"));

    assert_eq!(output.provider_requests["openai"].len(), 3);
}
