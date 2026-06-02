#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn uploads_single_agent_file_from_inline_content() {
    let output = TestRunner::workflow(fixtures::AGENT_SINGLE_FILE_DIRECTIVE)
        .provider("qwen", |provider| {
            provider.model("qwen-doc-turbo", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        assert!(messages.iter().any(|message| {
                            message.get("role").and_then(Value::as_str) == Some("system")
                                && message
                                    .get("content")
                                    .and_then(Value::as_str)
                                    .is_some_and(|content| content.starts_with("fileid://file-fe-test-"))
                        }));
                    })
                    .respond_text("single file");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute single agent file directive workflow");

    assert_eq!(output.output, json!({ "value": "single file" }));
}
