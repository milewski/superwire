use super::support::{request, request_with_input, TrackingModelProvider};
use crate::model::{ModelAssetSource, ModelPromptContent};
use crate::service::ExecutorService;
use serde_json::json;
use superwire_dsl::ModelAssetKind;
use superwire_macros::workflow_source;

#[tokio::test]
async fn renders_image_asset_from_instruction_template_into_model_request() {
    let workflow = workflow_source! {
        provider openai from openai {}

        model vision from openai {
            id: "gpt-vision"
            assets: ["image"]
        }

        input {
            image_url: string
        }

        agent analyzer {
            model: model.vision
            instruction: "Analyze this image: {{ asset input.image_url }}"
            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(
            workflow,
            json!({ "image_url": "https://example.com/frame.png" }),
        ))
        .await
        .expect("workflow with image asset should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    assert_eq!(request.prompt, "Analyze this image: ");
    assert!(matches!(&request.prompt_content[0], ModelPromptContent::Text(text) if text == "Analyze this image: "));
    assert!(matches!(
        &request.prompt_content[1],
        ModelPromptContent::Asset(asset)
            if asset.kind == ModelAssetKind::Image
                && matches!(&asset.source, ModelAssetSource::Url(url) if url == "https://example.com/frame.png")
    ));
}

#[tokio::test]
async fn renders_dynamic_video_asset_with_options_into_model_request() {
    let workflow = workflow_source! {
        provider google from google {}

        model video_model from google {
            id: "gemini-video"
            assets: ["video"]
        }

        agent analyzer {
            model: model.video_model
            dynamic {
                video: asset "https://example.com/demo.mp4" {
                    media_type: "video/mp4"
                    title: "Demo"
                }
            }
            instruction: "What happens here? {{ dynamic.video }}"
            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(workflow))
        .await
        .expect("workflow with dynamic video asset should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    assert_eq!(request.prompt, "What happens here? ");
    assert!(matches!(
        &request.prompt_content[1],
        ModelPromptContent::Asset(asset)
            if asset.kind == ModelAssetKind::Video
                && asset.media_type.as_deref() == Some("video/mp4")
                && asset.title.as_deref() == Some("Demo")
    ));
}

#[tokio::test]
async fn infers_video_media_type_from_asset_source() {
    let workflow = workflow_source! {
        provider openai from openai {}

        model video_model from openai {
            id: "qwen-plus"
            assets: ["video"]
        }

        agent analyzer {
            model: model.video_model

            dynamic {
                video: asset "https://uxspot.cn/images/2020/05/uxspot-hero-video-preview.mp4" {
                    type: "video"
                }
            }

            instruction: "What is on this video? {{ dynamic.video }}"

            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(workflow))
        .await
        .expect("workflow with video asset should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    assert!(matches!(
        &request.prompt_content[1],
        ModelPromptContent::Asset(asset)
            if asset.kind == ModelAssetKind::Video
                && asset.media_type.as_deref() == Some("video/mp4")
    ));
}

#[tokio::test]
async fn rejects_asset_kind_not_declared_by_model_profile() {
    let workflow = workflow_source! {
        provider openai from openai {}

        model text_model from openai {
            id: "gpt-text"
        }

        agent analyzer {
            model: model.text_model
            instruction: asset "https://example.com/frame.png"
            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let service = ExecutorService::new(TrackingModelProvider::new(vec![json!({ "result": "done" })]));

    let error = service
        .execute(request(workflow))
        .await
        .expect_err("model without image assets should reject image asset");

    assert!(error.to_string().contains("does not declare support"));
}

#[tokio::test]
async fn renders_array_plucked_video_assets_into_model_request() {
    let workflow = workflow_source! {
        provider openai from openai {}

        model video_model from openai {
            id: "qwen-plus"
            assets: ["video"]
        }

        input {
            attachments: [{
                video_url: string
            }]
        }

        agent analyzer {
            model: model.video_model
            dynamic {
                videos: asset input.attachments.*.video_url {
                    type: "video"
                }
            }
            instruction: "Summarize these videos: {{ dynamic.videos }}"
            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(
            workflow,
            json!({
                "attachments": [
                    { "video_url": "https://example.com/first.mp4" },
                    { "video_url": "https://example.com/second.mp4" }
                ]
            }),
        ))
        .await
        .expect("workflow with array-plucked video assets should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    assert_eq!(request.prompt, "Summarize these videos: ");
    assert_eq!(request.prompt_content.len(), 3);
    assert!(matches!(
        &request.prompt_content[1],
        ModelPromptContent::Asset(asset)
            if asset.kind == ModelAssetKind::Video
                && matches!(&asset.source, ModelAssetSource::Url(url) if url == "https://example.com/first.mp4")
    ));
    assert!(matches!(
        &request.prompt_content[2],
        ModelPromptContent::Asset(asset)
            if asset.kind == ModelAssetKind::Video
                && matches!(&asset.source, ModelAssetSource::Url(url) if url == "https://example.com/second.mp4")
    ));
}

#[tokio::test]
async fn renders_empty_non_asset_array_reference_as_text() {
    let workflow = workflow_source! {
        provider openai from openai {}

        model text_model from openai {
            id: "gpt-text"
        }

        input {
            notes: [string]
        }

        agent analyzer {
            model: model.text_model
            instruction: "Notes: {{ input.notes }}"
            output {
                result: string
            }
        }

        output {
            result: agent.analyzer.result
        }
    };
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(workflow, json!({ "notes": [] })))
        .await
        .expect("workflow with empty string array prompt reference should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    assert_eq!(request.prompt, "Notes: []");
    assert!(matches!(&request.prompt_content[0], ModelPromptContent::Text(text) if text == "Notes: []"));
}

#[tokio::test]
async fn rejects_array_pluck_on_non_array_reference() {
    let workflow = workflow_source! {
        input {
            attachment: {
                video_url: string
            }
        }

        output {
            urls: input.attachment.*.video_url
        }
    };
    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    let error = service
        .execute(request_with_input(
            workflow,
            json!({ "attachment": { "video_url": "https://example.com/first.mp4" } }),
        ))
        .await
        .expect_err("array pluck on object should fail");

    let error_message = error.to_string();

    assert!(
        error_message.contains("invalid_reference_path"),
        "expected invalid reference path error, got {error_message}"
    );
}
