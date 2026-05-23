use crate::support::runner::TestRunner;
use serde_json::{json, Value};
use superwire_core::workflow_source;

#[tokio::test]
async fn sends_image_asset_as_openai_compatible_content_block() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model qwen_plus from openai {
            id: "qwen-plus"
            assets: ["image"]
        }

        agent analyzer {
            model: model.qwen_plus

            dynamic {
                image: asset "https://fastly.picsum.photos/id/237/536/354.jpg?hmac=i0yVXW1ORpyCZpQ-CknuyV-jbtU7_x9EBQVhvT5aRr0"
            }

            instruction: "what is on this image??: {{ dynamic.image }}"

            output {
                result: string
            }
        }

        output {
            analyzer: agent.analyzer
        }
    };

    let output = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("qwen-plus", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        let user_message = messages
                            .iter()
                            .rev()
                            .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
                            .expect("user message should be present");
                        let content_parts = user_message
                            .get("content")
                            .and_then(Value::as_array)
                            .expect("asset prompt should use OpenAI content parts");

                        assert_eq!(
                            content_parts.first().and_then(|content_part| content_part.get("type")),
                            Some(&json!("text"))
                        );
                        assert_eq!(
                            content_parts.first().and_then(|content_part| content_part.get("text")),
                            Some(&json!("what is on this image??: "))
                        );
                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.get("type")),
                            Some(&json!("image_url"))
                        );
                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.pointer("/image_url/url")),
                            Some(&json!(
                                "https://fastly.picsum.photos/id/237/536/354.jpg?hmac=i0yVXW1ORpyCZpQ-CknuyV-jbtU7_x9EBQVhvT5aRr0"
                            ))
                        );
                    })
                    .respond_json(json!({ "result": "a dog" }));
            });
        })
        .run()
        .await
        .expect("workflow with OpenAI-compatible image asset should execute");

    assert_eq!(output.output, json!({ "analyzer": { "result": "a dog" } }));
}

#[tokio::test]
async fn sends_video_asset_as_openai_compatible_content_block() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model qwen_plus from openai {
            id: "qwen-plus"
            assets: ["video"]
        }

        agent analyzer {
            model: model.qwen_plus

            dynamic {
                video: asset "https://uxspot.cn/images/2020/05/uxspot-hero-video-preview.mp4" {
                    type: "video"
                }
            }

            instruction: "what is on this video??: {{ dynamic.video }}"

            output {
                result: string
            }
        }

        output {
            analyzer: agent.analyzer
        }
    };

    let output = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("qwen-plus", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        let user_message = messages
                            .iter()
                            .rev()
                            .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
                            .expect("user message should be present");
                        let content_parts = user_message
                            .get("content")
                            .and_then(Value::as_array)
                            .expect("asset prompt should use OpenAI content parts");

                        assert_eq!(
                            content_parts.first().and_then(|content_part| content_part.get("type")),
                            Some(&json!("text"))
                        );
                        assert_eq!(
                            content_parts.first().and_then(|content_part| content_part.get("text")),
                            Some(&json!("what is on this video??: "))
                        );
                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.get("type")),
                            Some(&json!("video_url"))
                        );
                        assert_eq!(
                            content_parts.get(1).and_then(|content_part| content_part.pointer("/video_url/url")),
                            Some(&json!("https://uxspot.cn/images/2020/05/uxspot-hero-video-preview.mp4"))
                        );
                    })
                    .respond_json(json!({ "result": "a video" }));
            });
        })
        .run()
        .await
        .expect("workflow with OpenAI-compatible video asset should execute");

    assert_eq!(output.output, json!({ "analyzer": { "result": "a video" } }));
}
