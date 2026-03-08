use std::sync::Arc;

use engine_ai_core::parse_workflow;
use engine_ai_core::providers::ollama::OllamaProvider;
use engine_ai_core::providers::registry::ProviderRegistry;
use engine_ai_core::validation::validate_workflow;
use serde_json::json;

#[tokio::main]
async fn main() {
    colog::init();

    let document = parse_workflow(include_str!("../workflows/11_workflow_input_output.engine.ai"))
        .expect("failed to parse workflow");

    validate_workflow(&document).expect("failed to validate workflow");

    let input = json!({
        "topic": "anything",
        "audience": "young people"
    });

    let mut registry = ProviderRegistry::default();
    registry.register("ollama", Arc::new(OllamaProvider));

    let output = engine_ai_core::execution::orchestrator::execute_workflow(&document, &registry, Some(&input))
        .await
        .expect("failed to execute workflow");

    println!();
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
