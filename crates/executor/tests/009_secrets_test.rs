#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn passes_secret_api_key_to_provider() {
    let run_output = TestRunner::workflow(fixtures::SECRETS)
        .secrets(secret!({ "api_key": "sk-test-123" }))
        .provider("openai", |provider| {
            provider.api_key("sk-test-123").model("model-a", |model| {
                model.turn().expect_prompt("Say hello.").respond_string("hello");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute secrets workflow");

    assert_eq!(run_output.output, json!({ "greeting": "hello" }));
}
