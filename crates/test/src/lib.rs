use serde::Deserialize;

mod macros;

// Integration tests for Engine AI workflows
// Tests are marked with #[ignore] by default to avoid requiring a running Ollama instance
// Run with: cargo test --package engine-ai-test -- --ignored
// Or run specific test: cargo test --package engine-ai-test test_basic_workflow -- --ignored
//
// Usage examples:
//   let result = try_workflow!("../workflows/basic.ai").await;
//   let output = workflow!("../workflows/basic.ai").await;
//
//   let inputs = input!(
//       topic: "Rust",
//       audience: "developers"
//   );
//   let result = try_workflow!("../workflows/input_output.ai", inputs).await;
//   let output = workflow!("../workflows/input_output.ai", inputs).await;

#[tokio::test]
#[ignore]
async fn test_basic_workflow() {
    let output = workflow!("../workflows/basic.ai").await;

    println!("{}", output);

    assert!(output.is_object());
    assert!(output.pointer("/greeting").unwrap().is_string());
    assert!(output.pointer("/greeting").unwrap().is_string());
}

#[tokio::test]
#[ignore]
async fn test_input_output_workflow() {
    let inputs = input!(
        topic: "Rust",
        audience: "developers"
    );

    let output = workflow!("../workflows/input_output.ai", inputs).await;

    assert!(output.is_object());

    let object = output.as_object().unwrap();
    assert_eq!(object.get("topic").unwrap(), "Rust");
    assert_eq!(object.get("audience").unwrap(), "developers");
    assert!(object.contains_key("summary"));
}

#[tokio::test]
#[ignore]
async fn test_with_try_workflow() {
    let inputs = input!(
        topic: "WebAssembly",
        audience: "systems programmers"
    );

    let result = try_workflow!("../workflows/input_output.ai", inputs).await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.is_object());

    let object = output.as_object().unwrap();
    assert_eq!(object.get("topic").unwrap(), "WebAssembly");
}
