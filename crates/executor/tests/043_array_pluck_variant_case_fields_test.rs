#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn plucks_variant_case_array_fields() {
    let output = TestRunner::workflow(fixtures::ARRAY_PLUCK_VARIANT_CASE_FIELDS)
        .mcp("local", |mcp_builder| {
            mcp_builder.tool("fetch_qualitative_question_answers", |tool_builder| {
                tool_builder
                    .input_schema(schema! { project_id: i64, task_types: Vec<String> })
                    .respond_json(json!({
                        "answers": [
                            {
                                "task_id": "task-1",
                                "participant_id": 7,
                                "answer": {
                                    "task_type": "video_recording",
                                    "attachments": [
                                        {
                                            "type": "video",
                                            "id": 100,
                                            "transcript": "first transcript",
                                            "url": "https://example.test/video-100.mp4"
                                        }
                                    ]
                                }
                            },
                            {
                                "task_id": "task-2",
                                "participant_id": 8,
                                "answer": {
                                    "task_type": "video_recording",
                                    "attachments": [
                                        {
                                            "type": "video",
                                            "id": 101,
                                            "transcript": "second transcript",
                                            "url": "https://example.test/video-101.mp4"
                                        }
                                    ]
                                }
                            }
                        ]
                    }));
            });
        })
        .run()
        .await
        .expect("variant case array pluck fixture should execute");

    assert_eq!(
        output.output,
        json!({
            "result": [
                {
                    "type": "video",
                    "id": 100,
                    "transcript": "first transcript",
                    "url": "https://example.test/video-100.mp4"
                },
                {
                    "type": "video",
                    "id": 101,
                    "transcript": "second transcript",
                    "url": "https://example.test/video-101.mp4"
                }
            ]
        })
    );
}
