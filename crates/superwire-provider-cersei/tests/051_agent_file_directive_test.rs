#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn uploads_agent_file_and_injects_file_id_message() {
    let output = TestRunner::workflow(fixtures::AGENT_FILE_DIRECTIVE)
        .provider("qwen", |provider| {
            provider.model("qwen-doc-turbo", |model| {
                model.turn().respond_json(json!({ "value": "first" }));
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
                    .respond_json(json!({ "value": "second" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute agent file directive workflow");

    assert_eq!(output.output, json!({ "value": "second" }));
}
