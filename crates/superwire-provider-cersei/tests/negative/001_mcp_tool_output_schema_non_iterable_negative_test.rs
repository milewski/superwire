use crate::support::fixtures;
use crate::support::runner::TestRunner;

#[tokio::test]
async fn fails_when_wire_output_schema_declares_non_iterable_values_field() {
    let run_error = TestRunner::workflow(fixtures::MCP_TOOL_OUTPUT_SCHEMA_OVERRIDE_NON_ITERABLE)
        .mcp("local", |mcp| {
            mcp.tool("fetch_numbers", |tool| {
                tool.input_schema(schema! {});
            });
        })
        .run_expect_error()
        .await;

    let error_message = run_error.error.to_string();

    assert!(
        error_message.contains("iterable") || error_message.contains("array"),
        "{error_message}"
    );
}
