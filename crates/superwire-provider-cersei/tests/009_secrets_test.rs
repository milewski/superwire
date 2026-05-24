#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_secret_api_key_to_provider() {
    let output = TestRunner::workflow(fixtures::SECRETS)
        .secrets(json!({ "api_key": "sk-test-123" }))
        .provider("openai", |provider| {
            provider.api_key("sk-test-123");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Say hello.").respond_json(json!({ "value": "hello" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute secrets workflow");

    assert_eq!(output.output, json!({ "greeting": "hello" }));
}
