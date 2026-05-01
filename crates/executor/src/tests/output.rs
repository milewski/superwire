use super::fixtures;
use serde_json::json;

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
    let output = execute!(fixtures::HARDCODED_OUTPUT, output: "agent value").await;
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
        output: { "name": "Alice", "age": 30, "role": "engineer" },
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
        output: {
            "string_value": "hello",
            "number_value": 42,
            "boolean_value": true,
            "nullable_string": null,
            "array": ["a", "b", "c"],
            "fixed_array": ["x", "y", "z"],
            "enum_value": "ready"
        },
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
