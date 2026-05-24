use super::fixtures;
use super::support::{request, service};
use serde_json::json;
use superwire_dsl::workflow_source;

#[tokio::test]
async fn string_interpolation_in_prompt() {
    let output = execute!(
        fixtures::STRING_INTERPOLATION,
        input: {
            "product_name": "SuperWidget",
            "audience": "developers"
        },
        output: { "title": "v1.0", "body": "New release!" },
        output: { "message": "Launch message" },
    )
    .await;
    assert_eq!(
        output,
        json!({ "title": "v1.0", "body": "New release!", "launch_message": "Launch message" })
    );
}

#[tokio::test]
async fn hardcoded_output_values() {
    let output = execute!(fixtures::HARDCODED_OUTPUT, output: { "value": "agent value" }).await;
    assert_eq!(
        output,
        json!({
            "hardcoded_string": "fixed-value",
            "hardcoded_number": 42,
            "hardcoded_boolean": true,
            "agent_value": "agent value"
        })
    );
}

#[tokio::test]
async fn nested_output_construction() {
    let output = execute!(
        fixtures::NESTED_OUTPUT,
        output: { "text": "All good", "confidence": 0.95 },
    )
    .await;
    assert_eq!(
        output,
        json!({
            "version": 2,
            "generated_by": "status_workflow",
            "report": {
                "source": "workflow",
                "overview": { "text": "All good" },
                "metrics": { "confidence": 0.95, "status": "ok" }
            }
        })
    );
}

#[tokio::test]
async fn mixed_output_with_agent_and_literals() {
    let output = execute!(
        fixtures::MIXED_OUTPUT,
        input: { "question": "What is the meaning of life?" },
        output: { "answer": "42", "confidence": 0.99, "sources": ["docs", "faq"] },
    )
    .await;
    assert_eq!(
        output,
        json!({
            "answer": "42",
            "confidence": 0.99,
            "sources": ["docs", "faq"],
            "metadata": { "workflow": "qa_pipeline", "version": 1 }
        })
    );
}

#[tokio::test]
async fn schema_output_with_field_access() {
    let output = execute!(
        fixtures::SCHEMA_OUTPUT,
        output: { "value": { "name": "Alice", "age": 30, "role": "engineer" } },
    )
    .await;
    assert_eq!(
        output,
        json!({
            "profile": { "name": "Alice", "age": 30, "role": "engineer" },
            "name": "Alice",
            "age": 30
        })
    );
}

#[tokio::test]
async fn complex_types_output() {
    let output = execute!(
        fixtures::COMPLEX_TYPES,
        output: { "value": {
            "string_value": "hello",
            "number_value": 42,
            "boolean_value": true,
            "nullable_string": null,
            "array": ["a", "b", "c"],
            "fixed_array": ["x", "y", "z"],
            "enum_value": "ready"
        } },
    )
    .await;
    assert_eq!(
        output,
        json!({
            "result": {
                "string_value": "hello",
                "number_value": 42,
                "boolean_value": true,
                "nullable_string": null,
                "array": ["a", "b", "c"],
                "fixed_array": ["x", "y", "z"],
                "enum_value": "ready"
            },
            "string_value": "hello",
            "number_value": 42,
            "boolean_value": true,
            "array": ["a", "b", "c"],
            "enum_value": "ready"
        })
    );
}

#[tokio::test]
async fn optional_chaining_with_present_value() {
    let output = execute!(
        fixtures::OPTIONAL_CHAINING,
        output: {
            "label": "test",
            "details": { "score": 95, "tags": ["fast", "reliable"] }
        },
    )
    .await;
    assert_eq!(output, json!({ "label": "test", "score": 95, "tags": ["fast", "reliable"] }));
}

#[tokio::test]
async fn optional_chaining_with_null_value() {
    let output = execute!(
        fixtures::OPTIONAL_CHAINING,
        output: { "label": "test", "details": null },
    )
    .await;
    assert_eq!(output, json!({ "label": "test", "score": null, "tags": null }));
}

#[tokio::test]
async fn array_pluck_allows_nested_null_values() {
    let workflow = workflow_source! {
        dynamic {
            data: [
                {
                    elements: [{ example: 1 }]
                },
                {
                    elements: [{ example: null }]
                },
            ]
        }

        output {
            analyzer: dynamic.data.*.elements.*.example
        }
    };

    let output = service(Vec::new())
        .execute(request(workflow))
        .await
        .expect("array pluck with nested null value should execute")
        .output;

    assert_eq!(output, json!({ "analyzer": [1, null] }));
}

#[tokio::test]
async fn array_pluck_filters_null_values_in_non_null_mode() {
    let workflow = workflow_source! {
        dynamic {
            data: [
                {
                    elements: [
                        { example: 1 },
                        { example: null },
                        { other: true },
                        { example: "two" },
                        { example: { anything: 123 } },
                    ]
                },
                null,
            ]
        }

        output {
            analyzer: dynamic.data.*.elements.**.example
        }
    };

    let output = service(Vec::new())
        .execute(request(workflow))
        .await
        .expect("non-null array pluck should filter null values")
        .output;

    assert_eq!(
        output,
        json!({
            "analyzer": [
                1,
                "two",
                { "anything": 123 }
            ]
        })
    );
}

#[tokio::test]
async fn strict_array_pluck_accepts_matching_non_null_values() {
    let workflow = workflow_source! {
        dynamic {
            data: [
                {
                    elements: [
                        { example: 1 },
                        { example: 2 },
                    ]
                }
            ]
        }

        output {
            analyzer: dynamic.data.*.elements.***.example
        }
    };

    let output = service(Vec::new())
        .execute(request(workflow))
        .await
        .expect("strict array pluck should accept matching non-null values")
        .output;

    assert_eq!(output, json!({ "analyzer": [1, 2] }));
}

#[tokio::test]
async fn strict_array_pluck_rejects_mixed_values() {
    let workflow = workflow_source! {
        dynamic {
            data: [
                {
                    elements: [
                        { example: 1 },
                        { example: "two" },
                    ]
                }
            ]
        }

        output {
            analyzer: dynamic.data.*.elements.***.example
        }
    };

    let error = service(Vec::new())
        .execute(request(workflow))
        .await
        .expect_err("strict array pluck should reject mixed values");
    let error_message = error.to_string();

    assert!(
        error_message.contains("invalid_reference_path") || error_message.contains("mixed array pluck value types"),
        "expected strict array pluck error, got {error_message}"
    );
}
