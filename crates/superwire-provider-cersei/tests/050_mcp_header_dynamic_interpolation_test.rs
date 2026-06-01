#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_dynamic_interpolation_in_mcp_headers() {
    let output = TestRunner::workflow(fixtures::MCP_HEADER_DYNAMIC_INTERPOLATION)
        .mcp("local", |_| {})
        .run()
        .await
        .expect("fixture runner should resolve dynamic interpolation in MCP headers");

    let list_tools_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.method == "tools/list")
        .expect("MCP discovery should list tools");

    assert_eq!(
        list_tools_request.headers.get("Authorization").map(String::as_str),
        Some("Bearer 000|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
    );

    assert_eq!(
        output.output,
        json!({
            "authorization": "Bearer 000|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
        })
    );
}
