use engine_ai_core::execution::ExecutionEngine;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

fn get_workflow_path(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("crates");
    path.push("example");
    path.push("workflows");
    path.push(name);
    path.to_str().unwrap().to_string()
}

#[tokio::test]
async fn test_basic_agent_execution() {
    let engine = ExecutionEngine::new();

    let result = engine.execute_workflow(&get_workflow_path("basic.ai")).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}

#[tokio::test]
async fn test_schema_validation() {
    let engine = ExecutionEngine::new();

    let result = engine.execute_workflow(&get_workflow_path("inline_schema.ai")).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}

#[tokio::test]
async fn test_enum_schema_execution() {
    let engine = ExecutionEngine::new();

    let result = engine.execute_workflow(&get_workflow_path("enum_schema.ai")).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    let weather = output.get("weather").expect("weather field should exist");

    let condition = weather.get("condition").expect("condition field should exist");
    assert!(condition.is_string());

    let condition_str = condition.as_str().unwrap();
    assert!(
        ["sunny", "rainy", "cloudy", "snowy"].contains(&condition_str),
        "Invalid condition: {condition_str}"
    );

    let temperature = weather.get("temperature").expect("temperature field should exist");
    assert!(temperature.is_number());
}

#[tokio::test]
#[ignore = "This test requires a running Ollama server"]
async fn test_input_output_blocks() {
    let engine = ExecutionEngine::new();

    let mut inputs = HashMap::new();
    inputs.insert("topic".to_string(), json!("artificial intelligence"));
    inputs.insert("audience".to_string(), json!("developers"));

    let result = engine
        .execute_workflow_with_inputs(&get_workflow_path("input_output.ai"), inputs)
        .await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());

    assert_eq!(
        output.get("topic").unwrap().as_str().unwrap(),
        "artificial intelligence"
    );
    assert_eq!(output.get("audience").unwrap().as_str().unwrap(), "developers");
}

#[tokio::test]
#[ignore = "LLM may refuse to generate random numbers"]
async fn test_for_each_execution() {
    let engine = ExecutionEngine::new();

    let result = engine.execute_workflow(&get_workflow_path("for_each.ai")).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}

#[tokio::test]
async fn test_parallel_execution() {
    let engine = ExecutionEngine::new();

    let result = engine
        .execute_workflow(&get_workflow_path("parallel_execution.ai"))
        .await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}

#[tokio::test]
async fn test_context_sharing() {
    let engine = ExecutionEngine::new();

    let result = engine.execute_workflow(&get_workflow_path("context_sharing.ai")).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}

#[tokio::test]
async fn test_inline_type_execution() {
    let engine = ExecutionEngine::new();

    let result = engine
        .execute_workflow(&get_workflow_path("simple_inline_type.ai"))
        .await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());
}
