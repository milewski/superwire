use std::fs;
use std::sync::Arc;

use engine_ai_core::parse_workflow;
use engine_ai_core::providers::ollama::OllamaProvider;
use engine_ai_core::providers::registry::ProviderRegistry;
use engine_ai_core::validation::validate_workflow;
use log::info;

#[tokio::main]
async fn main() {
    colog::init();

    let workflow_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/example/workflows/basic.engine.ai".into());

    let source = fs::read_to_string(&workflow_path).expect("failed to read workflow file");
    let document = parse_workflow(&source).expect("failed to parse workflow");
    validate_workflow(&document).expect("failed to validate workflow");

    let mut registry = ProviderRegistry::default();
    registry.register("ollama", Arc::new(OllamaProvider));

    info!("loaded workflow: {}", workflow_path);
    let output = engine_ai_core::execution::orchestrator::execute_workflow(&document, &registry)
        .await
        .expect("failed to execute workflow");
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
