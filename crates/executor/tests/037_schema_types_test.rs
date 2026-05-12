#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn supports_all_schema_types_and_variants() {
    let lead_participant = json!({ "name": "Ada", "role": "lead" });
    let participant_list = json!([
        { "name": "Ada", "role": "lead" },
        { "name": "Grace", "role": "reviewer" }
    ]);
    let previous_summary = json!({
        "title": "Prior research",
        "lead": { "name": "Ada", "role": "lead" },
        "participants": [
            { "name": "Ada", "role": "lead" },
            { "name": "Grace", "role": "reviewer" }
        ]
    });
    let typed_output = json!({
        "string_value": "hello",
        "number_value": 42,
        "float_value": 12.5,
        "boolean_value": true,
        "nullable_value": null,
        "nullable_string": "optional text",
        "nullable_number": null,
        "array": ["alpha", "beta"],
        "fixed_array": ["one", "two", "three"],
        "array_of_objects": [
            { "id": "item-1", "score": 98 },
            { "id": "item-2", "score": 87 }
        ],
        "enum_value": "ready",
        "nullable_enum": "published",
        "tuple_value": ["tuple", 7, ["x", "y", "z"]],
        "nullable_tuple": null,
        "object_value": {
            "string_value": "nested",
            "number_value": 99
        },
        "nullable_object": {
            "string_value": "nullable nested",
            "number_value": 100
        },
        "lead": { "name": "Ada", "role": "lead" },
        "participants": [
            { "name": "Ada", "role": "lead" },
            { "name": "Grace", "role": "reviewer" }
        ],
        "summary": {
            "title": "Current research",
            "lead": { "name": "Ada", "role": "lead" },
            "participants": [
                { "name": "Ada", "role": "lead" },
                { "name": "Grace", "role": "reviewer" }
            ]
        },
        "event": {
            "type": "created",
            "id": "event-1",
            "actor": { "name": "Grace", "role": "reviewer" }
        },
        "nullable_event": {
            "type": "deleted",
            "id": "event-2",
            "reason": "duplicate"
        }
    });

    let execution_output = TestRunner::workflow(fixtures::SCHEMA_TYPES)
        .input(json!({
            "lead_participant": lead_participant,
            "participant_list": participant_list,
            "previous_summary": previous_summary
        }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Generate a JSON object that matches the all_schema_types schema exactly for Ada")
                    .respond_json(typed_output.clone());
            });
        })
        .run()
        .await
        .expect("fixture runner should execute all schema types workflow");

    assert_eq!(
        execution_output.output,
        json!({
            "result": typed_output,
            "string_value": "hello",
            "number_value": 42,
            "float_value": 12.5,
            "boolean_value": true,
            "nullable_value": null,
            "nullable_string": "optional text",
            "nullable_number": null,
            "array": ["alpha", "beta"],
            "fixed_array": ["one", "two", "three"],
            "array_of_objects": [
                { "id": "item-1", "score": 98 },
                { "id": "item-2", "score": 87 }
            ],
            "enum_value": "ready",
            "nullable_enum": "published",
            "tuple_value": ["tuple", 7, ["x", "y", "z"]],
            "nullable_tuple": null,
            "object_value": {
                "string_value": "nested",
                "number_value": 99
            },
            "nested_string": "nested",
            "nested_number": 99,
            "nullable_object": {
                "string_value": "nullable nested",
                "number_value": 100
            },
            "nullable_object_string": "nullable nested",
            "lead": { "name": "Ada", "role": "lead" },
            "lead_name": "Ada",
            "participants": [
                { "name": "Ada", "role": "lead" },
                { "name": "Grace", "role": "reviewer" }
            ],
            "summary_title": "Current research",
            "summary_lead_role": "lead",
            "event_id": "event-1",
            "created_actor_name": "Grace",
            "deleted_reason": "not-deleted",
            "nullable_event_id": "event-2",
            "nullable_deleted_reason": "duplicate"
        })
    );
}
