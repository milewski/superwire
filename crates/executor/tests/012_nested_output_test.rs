#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn constructs_nested_output_values() {
    let output = TestRunner::workflow(fixtures::NESTED_OUTPUT)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write a project status summary with a confidence score.")
                    .respond_json(json!({ "text": "All good", "confidence": 0.95 }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute nested output workflow");

    assert_eq!(
        output.output,
        json!({
            "version": 2,
            "generated_by": "status_workflow",
            "report": {
                "source": "workflow",
                "overview": { "text": "All good" },
                "metrics": { "confidence": 0.95, "status": "ok" },
            },
        })
    );
}
