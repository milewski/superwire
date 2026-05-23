#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn plucks_array_fields_for_assets_and_for_loop_iterables() {
    let output = TestRunner::workflow(fixtures::ARRAY_PLUCK_ASSETS)
        .input(json!({
            "video_recording_answers": {
                "answers": [
                    { "url": "https://example.com/first.mp4" },
                    { "url": "https://example.com/second.mp4" }
                ]
            }
        }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        let content_parts = latest_user_content_parts(messages);

                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.pointer("/video_url/url")),
                            Some(&json!("https://example.com/first.mp4"))
                        );
                    })
                    .respond_json(json!({ "summary": "first" }));

                model
                    .turn()
                    .with_messages(|messages| {
                        let content_parts = latest_user_content_parts(messages);

                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.pointer("/video_url/url")),
                            Some(&json!("https://example.com/second.mp4"))
                        );
                    })
                    .respond_json(json!({ "summary": "second" }));

                model
                    .turn()
                    .with_messages(|messages| {
                        let content_parts = latest_user_content_parts(messages);

                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.pointer("/video_url/url")),
                            Some(&json!("https://example.com/first.mp4"))
                        );
                        assert_eq!(
                            content_parts.get(2).and_then(|content_part| content_part.pointer("/video_url/url")),
                            Some(&json!("https://example.com/second.mp4"))
                        );
                    })
                    .respond_json(json!({ "summary": "collection" }));
            });
        })
        .run()
        .await
        .expect("array pluck assets fixture should execute");

    assert_eq!(
        output.output,
        json!({
            "summaries": [
                { "url": "https://example.com/first.mp4", "summary": "first" },
                { "url": "https://example.com/second.mp4", "summary": "second" }
            ],
            "collection": { "summary": "collection" }
        })
    );
}

fn latest_user_content_parts(messages: &[Value]) -> &[Value] {
    messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .expect("latest user message should use content parts")
}
