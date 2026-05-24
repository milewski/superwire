#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_complex_output_types() {
    let typed_output = json!({
        "string_value": "hello",
        "number_value": 42,
        "boolean_value": true,
        "nullable_string": null,
        "array": ["a", "b", "c"],
        "fixed_array": ["x", "y", "z"],
        "enum_value": "ready",
    });

    let output = TestRunner::workflow(fixtures::COMPLEX_TYPES)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Generate a typed object.")
                    .respond_json(json!({ "value": typed_output.clone() }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute complex types workflow");

    assert_eq!(
        output.output,
        json!({
            "result": typed_output,
            "string_value": "hello",
            "number_value": 42,
            "boolean_value": true,
            "array": ["a", "b", "c"],
            "enum_value": "ready",
        })
    );
}
