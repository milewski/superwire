#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn interpolates_input_and_agent_output_in_prompts() {
    let run_output = TestRunner::workflow(fixtures::STRING_INTERPOLATION)
        .input(input!({ "product_name": "SuperWidget", "audience": "developers" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write release notes for SuperWidget targeting developers.")
                    .respond_json(json!({ "title": "v1.0", "body": "New release!" }));

                model
                    .turn()
                    .expect_prompt("Product: SuperWidget")
                    .expect_prompt("Title: v1.0")
                    .expect_prompt("Body: New release!")
                    .respond_json(json!({ "message": "Launch message" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute string interpolation workflow");

    assert_eq!(
        run_output.output,
        json!({ "title": "v1.0", "body": "New release!", "launch_message": "Launch message" })
    );
}
