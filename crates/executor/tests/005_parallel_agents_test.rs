#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_parallel_agents_fixture() {
    let output = TestRunner::workflow(fixtures::PARALLEL_AGENTS)
        .input(input!({ "product_name": "SuperWidget" }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Write release notes for SuperWidget.")
                    .respond_json(json!({ "markdown": "# Release Notes" }));

                model
                    .turn()
                    .expect_prompt("Create a launch thread for SuperWidget.")
                    .respond_json(json!({ "posts": ["post1", "post2"] }));

                model
                    .turn()
                    .expect_prompt("Write an announcement email for SuperWidget.")
                    .respond_json(json!({ "subject": "Launch!", "body": "We launched!" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute parallel agents workflow");

    assert_eq!(
        output.output,
        json!({
            "changelog": "# Release Notes",
            "posts": ["post1", "post2"],
            "email_subject": "Launch!",
            "email_body": "We launched!",
        })
    );
}
