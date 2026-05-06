#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn combines_agent_output_with_literal_metadata() {
    let output = TestRunner::workflow(fixtures::MIXED_OUTPUT)
        .input(input!({ "question": "What is the meaning of life?" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Answer: What is the meaning of life?")
                    .respond_json(json!({
                        "answer": "42",
                        "confidence": 0.99,
                        "sources": ["docs", "faq"],
                    }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute mixed output workflow");

    assert_eq!(
        output.output,
        json!({
            "answer": "42",
            "confidence": 0.99,
            "sources": ["docs", "faq"],
            "metadata": { "workflow": "qa_pipeline", "version": 1 },
        })
    );
}
