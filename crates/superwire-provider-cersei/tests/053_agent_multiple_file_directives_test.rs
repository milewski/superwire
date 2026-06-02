#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn uploads_multiple_agent_files_from_one_agent_body() {
    let output = TestRunner::workflow(fixtures::AGENT_MULTIPLE_FILE_DIRECTIVES)
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
                    .respond_text("multiple files");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute multiple agent file directives workflow");

    assert_eq!(output.output, json!({ "value": "multiple files" }));
}
