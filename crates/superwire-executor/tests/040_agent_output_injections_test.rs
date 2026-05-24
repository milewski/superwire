#[macro_use]
mod support;

use serde_json::{json, Map, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn injects_bound_and_dynamic_agent_output_values_after_finalize() {
    let output = TestRunner::workflow(fixtures::AGENT_OUTPUT_INJECTIONS)
        .input(json!({
            "details": {
                "enabled": true,
                "tags": ["dry", "short"],
            },
        }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Please create a joke about a cat").respond_json(json!({
                    "joke": "cat joke",
                    "subject": {
                        "theme": "cat",
                        "nested": {
                            "example": 10,
                        },
                    },
                }));

                model.turn().expect_prompt("Please create a joke about a dog").respond_json(json!({
                    "joke": "dog joke",
                    "subject": {
                        "theme": "dog",
                        "nested": {
                            "example": 20,
                        },
                    },
                }));
            });
        })
        .run()
        .await
        .expect("agent output injections should execute");

    let first_request = output.provider_requests["openai"].first().expect("provider request should exist");
    let finalize_parameters = find_finalize_parameters(first_request);
    let output_properties = schema_object_at(finalize_parameters, "/properties/output/properties");
    let subject_properties = schema_object_at(finalize_parameters, "/properties/output/properties/subject/properties");
    let nested_properties = schema_object_at(
        finalize_parameters,
        "/properties/output/properties/subject/properties/nested/properties",
    );

    assert!(output_properties.contains_key("joke"));
    assert!(output_properties.contains_key("subject"));
    assert!(!output_properties.contains_key("input_copy"));
    assert!(!output_properties.contains_key("dynamic_number"));

    assert!(subject_properties.contains_key("theme"));
    assert!(subject_properties.contains_key("nested"));
    assert!(!subject_properties.contains_key("something"));
    assert!(!subject_properties.contains_key("dynamic"));

    assert!(nested_properties.contains_key("example"));
    assert!(!nested_properties.contains_key("property"));
    assert!(!nested_properties.contains_key("enabled"));
    assert!(!nested_properties.contains_key("local_object"));

    assert_eq!(output.output, expected_output());
}

fn expected_output() -> Value {
    json!({
        "example": [
            {
                "joke": "cat joke",
                "subject": {
                    "theme": "cat",
                    "something": "hardcoded",
                    "dynamic": "a cat",
                    "nested": {
                        "example": 10,
                        "property": 42,
                        "enabled": true,
                        "local_object": {
                            "flag": true,
                            "tags": ["dry", "short"],
                        },
                    },
                },
                "input_copy": {
                    "enabled": true,
                    "tags": ["dry", "short"],
                },
                "dynamic_number": 7,
            },
            {
                "joke": "dog joke",
                "subject": {
                    "theme": "dog",
                    "something": "hardcoded",
                    "dynamic": "a dog",
                    "nested": {
                        "example": 20,
                        "property": 42,
                        "enabled": true,
                        "local_object": {
                            "flag": true,
                            "tags": ["dry", "short"],
                        },
                    },
                },
                "input_copy": {
                    "enabled": true,
                    "tags": ["dry", "short"],
                },
                "dynamic_number": 7,
            },
        ],
    })
}

fn find_finalize_parameters(request: &Value) -> &Value {
    request
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool.pointer("/function/name").and_then(Value::as_str) == Some("finalize"))
        })
        .and_then(|tool| tool.pointer("/function/parameters"))
        .expect("finalize parameters should exist")
}

fn schema_object_at<'value>(schema: &'value Value, path: &str) -> &'value Map<String, Value> {
    schema
        .pointer(path)
        .and_then(Value::as_object)
        .expect("schema path should be an object")
}
